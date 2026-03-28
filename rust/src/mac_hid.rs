//! mac_hid.rs -- Native macOS IOKit HID device backend for Logitech HID++.
//!
//! The `hidapi` crate on macOS only delivers standard mouse reports (report ID
//! 0x02) for BLE Logitech devices.  HID++ notification reports (0x10/0x11) are
//! silently dropped because `hidapi` opens only a single HID interface.
//!
//! This module uses IOKit's `IOHIDManager` / `IOHIDDevice` directly so that
//! **all** report types -- including HID++ -- are delivered via the input-report
//! callback.
//!
//! The public API mirrors the subset of `hidapi::HidDevice` used by the
//! `Worker` in `hid_gesture.rs`: `write`, `read_timeout`, and `close`.

#![cfg(target_os = "macos")]
#![allow(non_upper_case_globals, dead_code)]

use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr;
use std::sync::{Arc, Mutex};

use log::{info, warn};

// ── CoreFoundation / IOKit C types ──────────────────────────────────────────

type CFAllocatorRef = *const c_void;
type CFTypeRef = *const c_void;
type CFMutableDictionaryRef = *mut c_void;
type CFDictionaryRef = *const c_void;
type CFStringRef = *const c_void;
type CFNumberRef = *const c_void;
type CFSetRef = *const c_void;
type CFRunLoopRef = *const c_void;
type CFRunLoopMode = CFStringRef;
type CFIndex = isize;
type CFTimeInterval = f64;
type Boolean = u8;

type IOHIDManagerRef = *const c_void;
type IOHIDDeviceRef = *const c_void;
type IOReturn = i32;

const kCFNumberSInt32Type: u32 = 3;
const kCFStringEncodingUTF8: u32 = 0x0800_0100;
const kIOHIDReportTypeOutput: u32 = 1;
const kIOReturnSuccess: IOReturn = 0;

/// Callback signature for `IOHIDDeviceRegisterInputReportCallback`.
type IOHIDReportCallback = unsafe extern "C" fn(
    context: *mut c_void,
    result: IOReturn,
    sender: *mut c_void,
    report_type: u32,     // IOHIDReportType
    report_id: u32,
    report: *mut u8,
    report_length: CFIndex,
);

// ── CoreFoundation externs ──────────────────────────────────────────────────

extern "C" {
    static kCFRunLoopDefaultMode: CFRunLoopMode;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;

    fn CFRelease(cf: CFTypeRef);
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;

    fn CFNumberCreate(
        allocator: CFAllocatorRef,
        the_type: u32,
        value_ptr: *const c_void,
    ) -> CFNumberRef;
    fn CFNumberGetValue(
        number: CFNumberRef,
        the_type: u32,
        value_ptr: *mut c_void,
    ) -> Boolean;

    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c_str: *const std::ffi::c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut std::ffi::c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> Boolean;

    fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFDictionaryRef;
    fn CFDictionaryCreateMutable(
        allocator: CFAllocatorRef,
        capacity: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFMutableDictionaryRef;
    fn CFDictionarySetValue(
        the_dict: CFMutableDictionaryRef,
        key: *const c_void,
        value: *const c_void,
    );

    fn CFSetGetCount(the_set: CFSetRef) -> CFIndex;
    fn CFSetGetValues(the_set: CFSetRef, values: *mut *const c_void);

    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopRunInMode(
        mode: CFRunLoopMode,
        seconds: CFTimeInterval,
        return_after_source_handled: Boolean,
    ) -> i32;
}

// ── IOKit HID externs ───────────────────────────────────────────────────────

extern "C" {
    fn IOHIDManagerCreate(
        allocator: CFAllocatorRef,
        options: u32,
    ) -> IOHIDManagerRef;
    fn IOHIDManagerSetDeviceMatching(
        manager: IOHIDManagerRef,
        matching: CFDictionaryRef,
    );
    fn IOHIDManagerOpen(manager: IOHIDManagerRef, options: u32) -> IOReturn;
    fn IOHIDManagerCopyDevices(manager: IOHIDManagerRef) -> CFSetRef;

    fn IOHIDDeviceOpen(device: IOHIDDeviceRef, options: u32) -> IOReturn;
    fn IOHIDDeviceClose(device: IOHIDDeviceRef, options: u32) -> IOReturn;
    fn IOHIDDeviceGetProperty(
        device: IOHIDDeviceRef,
        key: CFStringRef,
    ) -> CFTypeRef;
    fn IOHIDDeviceSetReport(
        device: IOHIDDeviceRef,
        report_type: u32,
        report_id: CFIndex,
        report: *const u8,
        report_length: CFIndex,
    ) -> IOReturn;
    fn IOHIDDeviceRegisterInputReportCallback(
        device: IOHIDDeviceRef,
        report: *mut u8,
        report_length: CFIndex,
        callback: IOHIDReportCallback,
        context: *mut c_void,
    );
    fn IOHIDDeviceScheduleWithRunLoop(
        device: IOHIDDeviceRef,
        run_loop: CFRunLoopRef,
        run_loop_mode: CFRunLoopMode,
    );
    fn IOHIDDeviceUnscheduleFromRunLoop(
        device: IOHIDDeviceRef,
        run_loop: CFRunLoopRef,
        run_loop_mode: CFRunLoopMode,
    );
}

// ── CF helper wrappers ──────────────────────────────────────────────────────

/// Create a CFString from a Rust `&str`. Caller must `CFRelease`.
unsafe fn cfstring(s: &str) -> CFStringRef {
    let c = std::ffi::CString::new(s).unwrap();
    CFStringCreateWithCString(ptr::null(), c.as_ptr(), kCFStringEncodingUTF8)
}

/// Create a CFNumber (SInt32) from an `i32`. Caller must `CFRelease`.
unsafe fn cfnumber(val: i32) -> CFNumberRef {
    CFNumberCreate(
        ptr::null(),
        kCFNumberSInt32Type,
        &val as *const i32 as *const c_void,
    )
}

/// Read a SInt32 from a CFNumber.
unsafe fn cfnumber_to_i32(num: CFNumberRef) -> Option<i32> {
    if num.is_null() {
        return None;
    }
    let mut val: i32 = 0;
    let ok = CFNumberGetValue(num, kCFNumberSInt32Type, &mut val as *mut i32 as *mut c_void);
    if ok != 0 { Some(val) } else { None }
}

/// Read a UTF-8 string from a CFString.
unsafe fn cfstring_to_string(s: CFStringRef) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let mut buf = [0i8; 256];
    let ok = CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as CFIndex, kCFStringEncodingUTF8);
    if ok != 0 {
        let bytes: Vec<u8> = buf.iter().map(|&b| b as u8).take_while(|&b| b != 0).collect();
        Some(String::from_utf8_lossy(&bytes).into_owned())
    } else {
        None
    }
}

/// Get an integer property from an IOHIDDevice.
unsafe fn device_int_property(device: IOHIDDeviceRef, key: &str) -> Option<i32> {
    let k = cfstring(key);
    let val = IOHIDDeviceGetProperty(device, k);
    CFRelease(k);
    cfnumber_to_i32(val)
}

/// Get a string property from an IOHIDDevice.
unsafe fn device_string_property(device: IOHIDDeviceRef, key: &str) -> Option<String> {
    let k = cfstring(key);
    let val = IOHIDDeviceGetProperty(device, k);
    CFRelease(k);
    cfstring_to_string(val)
}

/// Build a mutable CFDictionary matching VendorID (and optionally more keys).
unsafe fn make_matching_dict(entries: &[(&str, i32)]) -> CFMutableDictionaryRef {
    let dict = CFDictionaryCreateMutable(
        ptr::null(),
        entries.len() as CFIndex,
        &kCFTypeDictionaryKeyCallBacks as *const c_void,
        &kCFTypeDictionaryValueCallBacks as *const c_void,
    );
    for &(key, val) in entries {
        let k = cfstring(key);
        let v = cfnumber(val);
        CFDictionarySetValue(dict, k as *const c_void, v as *const c_void);
        CFRelease(k);
        CFRelease(v);
    }
    dict
}

// ── Public device info struct ───────────────────────────────────────────────

/// Metadata about one IOKit HID device interface.
#[derive(Debug, Clone)]
pub struct MacHidDeviceInfo {
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub transport: Option<String>,
    pub product_string: String,
    /// Opaque: the `IOHIDDeviceRef` retained at enumerate time so we can
    /// open the exact same interface later.  Not user-visible.
    device_ref: usize, // stored as usize to be Send
}

// ── Enumerator ──────────────────────────────────────────────────────────────

pub struct MacHidEnumerator;

impl MacHidEnumerator {
    /// Enumerate Logitech HID devices with usage_page >= `usage_page_min`.
    ///
    /// Tries multiple matching strategies:
    /// 1. First, match by VendorID only and filter ourselves
    /// 2. If that fails, try per-usage-page matching for known HID++ pages
    pub fn enumerate(vendor_id: u16, usage_page_min: u16) -> Vec<MacHidDeviceInfo> {
        let mut infos = Vec::new();
        unsafe {
            // Use NULL matching (like hidapi does) to get ALL HID devices,
            // then filter ourselves. IOHIDManager with specific matching
            // dicts does NOT expose vendor-specific BLE sub-interfaces.
            let dict: CFMutableDictionaryRef = ptr::null_mut();
            let manager = IOHIDManagerCreate(ptr::null(), 0);
            if manager.is_null() {
                warn!("[MacHid] IOHIDManagerCreate failed");
                CFRelease(dict as CFTypeRef);
                return infos;
            }
            IOHIDManagerSetDeviceMatching(manager, dict as CFDictionaryRef); // NULL = all devices
            let ret = IOHIDManagerOpen(manager, 0);
            if ret != kIOReturnSuccess {
                warn!("[MacHid] IOHIDManagerOpen failed: 0x{ret:08X}");
                CFRelease(manager as CFTypeRef);
                return infos;
            }

            let devices = IOHIDManagerCopyDevices(manager);
            if !devices.is_null() {
                let count = CFSetGetCount(devices);
                if count > 0 {
                    let mut vals = vec![ptr::null(); count as usize];
                    CFSetGetValues(devices, vals.as_mut_ptr());

                    let mut seen = std::collections::HashSet::new();
                    for &dev_ref in &vals {
                        let vid = device_int_property(dev_ref as IOHIDDeviceRef, "VendorID")
                            .unwrap_or(0) as u16;
                        if vid != vendor_id {
                            continue;
                        }
                        let pid = device_int_property(dev_ref as IOHIDDeviceRef, "ProductID")
                            .unwrap_or(0) as u16;
                        let up = device_int_property(dev_ref as IOHIDDeviceRef, "PrimaryUsagePage")
                            .unwrap_or(0) as u16;
                        let usage = device_int_property(dev_ref as IOHIDDeviceRef, "PrimaryUsage")
                            .unwrap_or(0) as u16;
                        let transport =
                            device_string_property(dev_ref as IOHIDDeviceRef, "Transport");
                        let product = device_string_property(dev_ref as IOHIDDeviceRef, "Product")
                            .unwrap_or_default();

                        info!(
                            "[MacHid] raw device: PID=0x{:04X} UP=0x{:04X} usage=0x{:04X} transport={:?} product={}",
                            pid, up, usage, transport.as_deref().unwrap_or("?"), product
                        );
                        if pid == 0 {
                            continue;
                        }
                        // Only accept actual HID++ interfaces (UP >= 0xFF00).
                        // BLE devices where HID++ is merged into UP=0x0001
                        // must use open_ble() instead (which keeps the manager
                        // alive — required for async report delivery).
                        if pid == 0 || up < usage_page_min {
                            continue;
                        }
                        let key = (pid, up, usage, transport.clone().unwrap_or_default(), product.clone());
                        if !seen.insert(key) {
                            continue;
                        }

                        // Retain the device ref so we can open it later
                        CFRetain(dev_ref);

                        infos.push(MacHidDeviceInfo {
                            product_id: pid,
                            usage_page: up,
                            usage,
                            transport,
                            product_string: product,
                            device_ref: dev_ref as usize,
                        });
                    }
                }
                CFRelease(devices);
            }

            // dict is NULL, don't release it
            CFRelease(manager as CFTypeRef);
        }
        infos
    }

    /// Enumerate BLE Logitech devices by matching `Transport=Bluetooth Low Energy`.
    /// On macOS BLE, the HID++ sub-interface isn't exposed separately — macOS
    /// merges everything into one device.  We open it without usage_page filtering
    /// and register for ALL report types (including HID++ 0x10/0x11).
    pub fn enumerate_ble(vendor_id: u16) -> Vec<MacHidDeviceInfo> {
        let mut infos = Vec::new();
        unsafe {
            // Match by VendorID + Transport
            let dict = CFDictionaryCreateMutable(
                ptr::null(),
                2,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            let k_vid = cfstring("VendorID");
            let v_vid = cfnumber(vendor_id as i32);
            CFDictionarySetValue(dict, k_vid as *const c_void, v_vid as *const c_void);
            CFRelease(k_vid);
            CFRelease(v_vid);

            let k_transport = cfstring("Transport");
            let v_transport = cfstring("Bluetooth Low Energy");
            CFDictionarySetValue(dict, k_transport as *const c_void, v_transport as *const c_void);
            CFRelease(k_transport);
            CFRelease(v_transport);

            let manager = IOHIDManagerCreate(ptr::null(), 0);
            if manager.is_null() {
                CFRelease(dict as CFTypeRef);
                return infos;
            }
            IOHIDManagerSetDeviceMatching(manager, dict as CFDictionaryRef);
            let ret = IOHIDManagerOpen(manager, 0);
            if ret != kIOReturnSuccess {
                CFRelease(dict as CFTypeRef);
                CFRelease(manager as CFTypeRef);
                return infos;
            }

            let devices = IOHIDManagerCopyDevices(manager);
            if !devices.is_null() {
                let count = CFSetGetCount(devices);
                if count > 0 {
                    let mut vals = vec![ptr::null(); count as usize];
                    CFSetGetValues(devices, vals.as_mut_ptr());

                    for &dev_ref in &vals {
                        let pid = device_int_property(dev_ref as IOHIDDeviceRef, "ProductID")
                            .unwrap_or(0) as u16;
                        let up = device_int_property(dev_ref as IOHIDDeviceRef, "PrimaryUsagePage")
                            .unwrap_or(0) as u16;
                        let usage = device_int_property(dev_ref as IOHIDDeviceRef, "PrimaryUsage")
                            .unwrap_or(0) as u16;
                        let product = device_string_property(dev_ref as IOHIDDeviceRef, "Product")
                            .unwrap_or_default();
                        let transport = device_string_property(dev_ref as IOHIDDeviceRef, "Transport");

                        info!(
                            "[MacHid] BLE device: PID=0x{:04X} UP=0x{:04X} usage=0x{:04X} transport={:?} product={}",
                            pid, up, usage, transport.as_deref().unwrap_or("?"), product
                        );

                        if pid == 0 {
                            continue;
                        }

                        CFRetain(dev_ref);
                        infos.push(MacHidDeviceInfo {
                            product_id: pid,
                            usage_page: up,
                            usage,
                            transport,
                            product_string: product,
                            device_ref: dev_ref as usize,
                        });
                    }
                }
                CFRelease(devices);
            }

            CFRelease(dict as CFTypeRef);
            CFRelease(manager as CFTypeRef);
        }
        infos
    }

    /// Open a specific device from enumeration results.
    pub fn open(info: &MacHidDeviceInfo) -> Result<MacNativeHidDevice, String> {
        MacNativeHidDevice::open_from_ref(info)
    }
}

// ── Report queue context ────────────────────────────────────────────────────

/// Shared state between the input-report callback and `read_timeout`.
struct ReportQueue {
    queue: VecDeque<Vec<u8>>,
}

// ── MacNativeHidDevice ──────────────────────────────────────────────────────

/// A native macOS IOKit HID device handle that receives ALL HID report types,
/// including HID++ notification reports (0x10/0x11).
pub struct MacNativeHidDevice {
    device: IOHIDDeviceRef,   // retained; we release in close()
    run_loop: CFRunLoopRef,
    /// The IOHIDManager that owns this device.  For BLE devices opened via
    /// `open_ble`, the manager MUST stay alive (retained) for the entire
    /// lifetime of the device -- releasing it invalidates the device ref.
    /// For devices opened via `open_from_ref` (enumerate path), this is None
    /// because the enumerator's manager has already been released.
    manager: Option<IOHIDManagerRef>,
    /// Heap-allocated input buffer for IOKit to write into.  Must live as long
    /// as the callback registration.
    input_buffer: *mut u8,
    input_buffer_len: usize,
    report_queue: Arc<Mutex<ReportQueue>>,
    /// We must prevent the closure / fn-pointer from being dropped while the
    /// callback is registered.  Storing the raw context pointer here.
    _callback_ctx: *mut Arc<Mutex<ReportQueue>>,
    closed: bool,
}

// Safety: The IOHIDDevice is only accessed from the thread that created it
// (the HidGesture worker thread) and CFRunLoop is pumped on that same thread.
unsafe impl Send for MacNativeHidDevice {}

impl MacNativeHidDevice {
    fn open_from_ref(info: &MacHidDeviceInfo) -> Result<Self, String> {
        unsafe {
            let device = info.device_ref as IOHIDDeviceRef;
            let ret = IOHIDDeviceOpen(device, 0);
            if ret != kIOReturnSuccess {
                return Err(format!(
                    "IOHIDDeviceOpen failed: 0x{ret:08X} for PID=0x{:04X}",
                    info.product_id
                ));
            }

            let run_loop = CFRunLoopGetCurrent();

            // Heap-allocate the input buffer (64 bytes, enough for long reports)
            let input_buffer_len: usize = 64;
            let input_buffer = {
                let layout = std::alloc::Layout::from_size_align(input_buffer_len, 1).unwrap();
                let ptr = std::alloc::alloc_zeroed(layout);
                if ptr.is_null() {
                    IOHIDDeviceClose(device, 0);
                    return Err("Failed to allocate input buffer".into());
                }
                ptr
            };

            let queue = Arc::new(Mutex::new(ReportQueue {
                queue: VecDeque::new(),
            }));

            // Put the Arc on the heap so we can pass a stable pointer to the C callback
            let ctx_box = Box::new(queue.clone());
            let ctx_ptr = Box::into_raw(ctx_box);

            // Schedule with run loop BEFORE registering callback
            IOHIDDeviceScheduleWithRunLoop(device, run_loop, kCFRunLoopDefaultMode);

            // Register the input-report callback
            IOHIDDeviceRegisterInputReportCallback(
                device,
                input_buffer,
                input_buffer_len as CFIndex,
                native_report_callback,
                ctx_ptr as *mut c_void,
            );

            info!(
                "[MacHid] Opened PID=0x{:04X} UP=0x{:04X} usage=0x{:04X} transport={:?}",
                info.product_id, info.usage_page, info.usage, info.transport
            );

            Ok(MacNativeHidDevice {
                device,
                run_loop,
                manager: None,
                input_buffer,
                input_buffer_len,
                report_queue: queue,
                _callback_ctx: ctx_ptr,
                closed: false,
            })
        }
    }

    /// Open a BLE Logitech device directly, matching Python's working approach.
    ///
    /// Creates a **fresh IOHIDManager** with an immutable matching dictionary
    /// (VendorID + ProductID + Transport="Bluetooth Low Energy"), opens the
    /// first matching device, and keeps the manager alive for the lifetime of
    /// the returned handle.  This mirrors the Python `_IOKitHidTransport.open`
    /// method which is known to work for BLE HID++ communication.
    pub fn open_ble(vendor_id: u16, product_id: u16) -> Result<Self, String> {
        unsafe {
            // Build an immutable CFDictionary with VendorID + ProductID + Transport
            let k_vid = cfstring("VendorID");
            let v_vid = cfnumber(vendor_id as i32);
            let k_pid = cfstring("ProductID");
            let v_pid = cfnumber(product_id as i32);
            let k_transport = cfstring("Transport");
            let v_transport = cfstring("Bluetooth Low Energy");

            let keys: [*const c_void; 3] = [
                k_vid as *const c_void,
                k_pid as *const c_void,
                k_transport as *const c_void,
            ];
            let values: [*const c_void; 3] = [
                v_vid as *const c_void,
                v_pid as *const c_void,
                v_transport as *const c_void,
            ];

            // Use NULL callbacks like Python does — this exactly matches
            // _MacNativeHidDevice.open() which passes None, None.
            let dict = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                3,
                ptr::null(),  // Python passes None (NULL) for key callbacks
                ptr::null(),  // Python passes None (NULL) for value callbacks
            );

            // DON'T release keys/values yet — with NULL callbacks the dict
            // does NOT retain them, so they must stay alive until we're done.
            // We release them after IOHIDManagerSetDeviceMatching + CopyDevices.

            if dict.is_null() {
                return Err("CFDictionaryCreate failed".into());
            }

            // Create a fresh IOHIDManager
            let manager = IOHIDManagerCreate(ptr::null(), 0);
            if manager.is_null() {
                CFRelease(dict);
                return Err("IOHIDManagerCreate failed".into());
            }

            IOHIDManagerSetDeviceMatching(manager, dict);
            // Now release dict and the keys/values
            CFRelease(dict);
            CFRelease(k_vid); CFRelease(v_vid);
            CFRelease(k_pid); CFRelease(v_pid);
            CFRelease(k_transport); CFRelease(v_transport);

            let ret = IOHIDManagerOpen(manager, 0);
            if ret != kIOReturnSuccess {
                CFRelease(manager as CFTypeRef);
                return Err(format!("IOHIDManagerOpen failed: 0x{ret:08X}"));
            }

            let devices = IOHIDManagerCopyDevices(manager);
            if devices.is_null() {
                CFRelease(manager as CFTypeRef);
                return Err(format!(
                    "No BLE devices matched VID=0x{vendor_id:04X} PID=0x{product_id:04X}"
                ));
            }

            let count = CFSetGetCount(devices);
            if count <= 0 {
                CFRelease(devices);
                CFRelease(manager as CFTypeRef);
                return Err(format!(
                    "No BLE devices matched VID=0x{vendor_id:04X} PID=0x{product_id:04X}"
                ));
            }

            let mut vals = vec![ptr::null(); count as usize];
            CFSetGetValues(devices, vals.as_mut_ptr());
            let dev_ref = vals[0];
            CFRetain(dev_ref);
            CFRelease(devices);

            let device = dev_ref as IOHIDDeviceRef;

            // Log what we matched
            let up = device_int_property(device, "PrimaryUsagePage").unwrap_or(0) as u16;
            let usage = device_int_property(device, "PrimaryUsage").unwrap_or(0) as u16;
            let product = device_string_property(device, "Product").unwrap_or_default();
            info!(
                "[MacHid] open_ble: matched PID=0x{:04X} UP=0x{:04X} usage=0x{:04X} product={}",
                product_id, up, usage, product
            );

            // Open the device
            let ret = IOHIDDeviceOpen(device, 0);
            if ret != kIOReturnSuccess {
                CFRelease(dev_ref);
                CFRelease(manager as CFTypeRef);
                return Err(format!(
                    "IOHIDDeviceOpen failed: 0x{ret:08X} for PID=0x{product_id:04X}"
                ));
            }

            let run_loop = CFRunLoopGetCurrent();

            // Allocate input buffer
            let input_buffer_len: usize = 64;
            let input_buffer = {
                let layout = std::alloc::Layout::from_size_align(input_buffer_len, 1).unwrap();
                let ptr = std::alloc::alloc_zeroed(layout);
                if ptr.is_null() {
                    IOHIDDeviceClose(device, 0);
                    CFRelease(dev_ref);
                    CFRelease(manager as CFTypeRef);
                    return Err("Failed to allocate input buffer".into());
                }
                ptr
            };

            let queue = Arc::new(Mutex::new(ReportQueue {
                queue: VecDeque::new(),
            }));
            let ctx_box = Box::new(queue.clone());
            let ctx_ptr = Box::into_raw(ctx_box);

            // Schedule with run loop, then register callback
            IOHIDDeviceScheduleWithRunLoop(device, run_loop, kCFRunLoopDefaultMode);
            IOHIDDeviceRegisterInputReportCallback(
                device,
                input_buffer,
                input_buffer_len as CFIndex,
                native_report_callback,
                ctx_ptr as *mut c_void,
            );

            info!(
                "[MacHid] open_ble: opened PID=0x{:04X} (manager retained)",
                product_id
            );

            Ok(MacNativeHidDevice {
                device,
                run_loop,
                manager: Some(manager),
                input_buffer,
                input_buffer_len,
                report_queue: queue,
                _callback_ctx: ctx_ptr,
                closed: false,
            })
        }
    }

    /// Write an output report. `data[0]` is the report ID.
    ///
    /// On macOS, `IOHIDDeviceSetReport` expects:
    /// - `report_id`: the HID report ID (data[0])
    /// - `report`: pointer to full data (including report_id byte)
    /// - `reportLength`: total length (same as hidapi C behaviour for report_id != 0)
    pub fn write(&self, data: &[u8]) -> Result<usize, String> {
        if self.closed || data.is_empty() {
            return Err("Device closed or empty data".into());
        }
        let report_id = data[0] as CFIndex;

        // Try with full buffer first (includes report_id byte, like hidapi C),
        // then without report_id byte (just the payload).
        // BLE devices on macOS sometimes need the report_id stripped.
        let attempts: &[(&[u8], &str)] = &[
            (data, "full"),
            (&data[1..], "stripped"),
        ];

        for &(buf, label) in attempts {
            log::debug!(
                "[MacHid] write({}) {} bytes rid=0x{:02X}: [{}]",
                label, buf.len(), data[0],
                buf.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
            );
            unsafe {
                let ret = IOHIDDeviceSetReport(
                    self.device,
                    kIOHIDReportTypeOutput,
                    report_id,
                    buf.as_ptr(),
                    buf.len() as CFIndex,
                );
                if ret == kIOReturnSuccess {
                    return Ok(data.len());
                }
                log::debug!("[MacHid] write({}) failed: 0x{ret:08X}", label);
            }
        }
        Err("IOHIDDeviceSetReport failed with both full and stripped buffers".into())
    }

    /// Read one report with timeout.  Pumps the CFRunLoop in 50ms slices to
    /// receive callbacks.  Returns the report bytes (WITHOUT the report-ID
    /// prefix, matching the Python implementation).
    ///
    /// Returns `Ok(0)` on timeout, `Ok(n)` when a report is copied into `buf`.
    pub fn read_timeout(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize, String> {
        if self.closed {
            return Err("Device closed".into());
        }

        // Check queue first
        {
            let mut q = self.report_queue.lock().unwrap();
            if let Some(report) = q.queue.pop_front() {
                let n = report.len().min(buf.len());
                buf[..n].copy_from_slice(&report[..n]);
                return Ok(n);
            }
        }

        let deadline = if timeout_ms > 0 {
            Some(std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64))
        } else if timeout_ms == 0 {
            // Non-blocking: pump once and return
            unsafe {
                CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.0, 1);
            }
            let mut q = self.report_queue.lock().unwrap();
            if let Some(report) = q.queue.pop_front() {
                let n = report.len().min(buf.len());
                buf[..n].copy_from_slice(&report[..n]);
                return Ok(n);
            }
            return Ok(0);
        } else {
            // Negative timeout = block indefinitely (use large deadline)
            None
        };

        loop {
            let slice_secs = if let Some(dl) = deadline {
                let remaining = dl.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    return Ok(0);
                }
                remaining.as_secs_f64().min(0.05)
            } else {
                0.05
            };

            unsafe {
                CFRunLoopRunInMode(kCFRunLoopDefaultMode, slice_secs, 1);
            }

            {
                let mut q = self.report_queue.lock().unwrap();
                if let Some(report) = q.queue.pop_front() {
                    let n = report.len().min(buf.len());
                    buf[..n].copy_from_slice(&report[..n]);
                    return Ok(n);
                }
            }

            if let Some(dl) = deadline {
                if std::time::Instant::now() >= dl {
                    return Ok(0);
                }
            }
        }
    }

    /// Close the device and clean up.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        unsafe {
            // Unschedule from run loop
            IOHIDDeviceUnscheduleFromRunLoop(self.device, self.run_loop, kCFRunLoopDefaultMode);

            // Deregister callback by registering a null callback
            IOHIDDeviceRegisterInputReportCallback(
                self.device,
                self.input_buffer,
                self.input_buffer_len as CFIndex,
                null_report_callback,
                ptr::null_mut(),
            );

            // Close the device
            IOHIDDeviceClose(self.device, 0);

            // Release the retained device ref
            CFRelease(self.device as CFTypeRef);

            // Release the IOHIDManager if we own one (open_ble path)
            if let Some(mgr) = self.manager.take() {
                CFRelease(mgr as CFTypeRef);
            }

            // Free the input buffer
            let layout =
                std::alloc::Layout::from_size_align(self.input_buffer_len, 1).unwrap();
            std::alloc::dealloc(self.input_buffer, layout);

            // Reclaim the context Box
            let _ = Box::from_raw(self._callback_ctx);
        }
    }
}

impl Drop for MacNativeHidDevice {
    fn drop(&mut self) {
        if !self.closed {
            self.close();
        }
    }
}

// ── C callbacks ─────────────────────────────────────────────────────────────

/// The input-report callback invoked by IOKit for EVERY report type.
///
/// `context` points to a `Box<Arc<Mutex<ReportQueue>>>`.
/// The report data does NOT include the report-ID byte (matching Python behavior).
unsafe extern "C" fn native_report_callback(
    context: *mut c_void,
    result: IOReturn,
    _sender: *mut c_void,
    _report_type: u32,
    report_id: u32,
    report: *mut u8,
    report_length: CFIndex,
) {
    if result != kIOReturnSuccess || report_length <= 0 || context.is_null() {
        return;
    }

    let queue_arc = &*(context as *const Arc<Mutex<ReportQueue>>);

    // IOKit's input buffer already includes the report_id as the first byte.
    // Do NOT prepend it again — that causes a double report_id which makes
    // parse_report misinterpret valid responses as errors.
    let data = std::slice::from_raw_parts(report, report_length as usize).to_vec();

    if let Ok(mut q) = queue_arc.lock() {
        // Cap queue size to prevent unbounded growth
        if q.queue.len() < 256 {
            q.queue.push_back(data);
        }
    }
}

/// Null callback used to deregister the real callback on close.
unsafe extern "C" fn null_report_callback(
    _context: *mut c_void,
    _result: IOReturn,
    _sender: *mut c_void,
    _report_type: u32,
    _report_id: u32,
    _report: *mut u8,
    _report_length: CFIndex,
) {
    // intentionally empty
}

// ── Release retained device refs when MacHidDeviceInfo is dropped ───────────

impl Drop for MacHidDeviceInfo {
    fn drop(&mut self) {
        if self.device_ref != 0 {
            unsafe {
                CFRelease(self.device_ref as CFTypeRef);
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_does_not_crash() {
        // Just verify the enumerate path doesn't panic/segfault.
        // On CI without a mouse attached this returns an empty vec.
        let infos = MacHidEnumerator::enumerate(0x046D, 0xFF00);
        // We can't assert on count, but we can check it doesn't panic.
        println!("[test] Found {} native IOKit HID devices", infos.len());
    }
}
