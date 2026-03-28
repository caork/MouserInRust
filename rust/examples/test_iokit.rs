/// Minimal IOKit HID++ test matching Python's _MacNativeHidDevice exactly.
/// Usage: cargo run --example test_iokit

#[cfg(target_os = "macos")]
fn main() {
    use std::ffi::c_void;
    use std::ptr;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    type CFIndex = isize;
    type CFTimeInterval = f64;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRelease(cf: *const c_void);
        fn CFRetain(cf: *const c_void) -> *const c_void;
        fn CFRunLoopGetCurrent() -> *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: CFTimeInterval, returnAfterSourceHandled: u8) -> i32;
        fn CFNumberCreate(alloc: *const c_void, theType: u32, valuePtr: *const c_void) -> *const c_void;
        fn CFStringCreateWithCString(alloc: *const c_void, cStr: *const i8, encoding: u32) -> *const c_void;
        fn CFStringGetCString(s: *const c_void, buf: *mut i8, bufSize: CFIndex, enc: u32) -> u8;
        fn CFDictionaryCreate(alloc: *const c_void, keys: *const *const c_void, vals: *const *const c_void, numVals: CFIndex, keyCb: *const c_void, valCb: *const c_void) -> *const c_void;
        fn CFSetGetCount(set: *const c_void) -> CFIndex;
        fn CFSetGetValues(set: *const c_void, values: *mut *const c_void);
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOHIDManagerCreate(alloc: *const c_void, options: u32) -> *mut c_void;
        fn IOHIDManagerSetDeviceMatching(mgr: *mut c_void, matching: *const c_void);
        fn IOHIDManagerOpen(mgr: *mut c_void, options: u32) -> i32;
        fn IOHIDManagerCopyDevices(mgr: *mut c_void) -> *const c_void;
        fn IOHIDDeviceOpen(dev: *const c_void, options: u32) -> i32;
        fn IOHIDDeviceClose(dev: *const c_void, options: u32) -> i32;
        fn IOHIDDeviceGetProperty(dev: *const c_void, key: *const c_void) -> *const c_void;
        fn IOHIDDeviceSetReport(dev: *const c_void, reportType: u32, reportID: CFIndex, report: *const u8, reportLength: CFIndex) -> i32;
        fn IOHIDDeviceScheduleWithRunLoop(dev: *const c_void, rl: *const c_void, mode: *const c_void);
        fn IOHIDDeviceRegisterInputReportCallback(dev: *const c_void, report: *mut u8, reportLength: CFIndex, callback: extern "C" fn(*mut c_void, i32, *mut c_void, u32, u32, *mut u8, CFIndex), context: *mut c_void);
    }

    static QUEUE: std::sync::OnceLock<Arc<Mutex<VecDeque<Vec<u8>>>>> = std::sync::OnceLock::new();

    extern "C" fn report_cb(_ctx: *mut c_void, result: i32, _sender: *mut c_void, _rt: u32, report_id: u32, report: *mut u8, len: CFIndex) {
        if result != 0 || len <= 0 { return; }
        let q = QUEUE.get().unwrap();
        // IOKit data already includes report_id as first byte. Don't prepend.
        let data = unsafe { std::slice::from_raw_parts(report, len as usize) };
        let v = data.to_vec();
        q.lock().unwrap().push_back(v);
    }

    unsafe fn cfstr(s: &str) -> *const c_void {
        let c = std::ffi::CString::new(s).unwrap();
        CFStringCreateWithCString(ptr::null(), c.as_ptr(), 0x08000100)
    }
    unsafe fn cfnum(v: i32) -> *const c_void {
        CFNumberCreate(ptr::null(), 3, &v as *const i32 as *const c_void)
    }
    unsafe fn get_int(dev: *const c_void, key: &str) -> i32 {
        let k = cfstr(key);
        let v = IOHIDDeviceGetProperty(dev, k);
        CFRelease(k);
        if v.is_null() { return 0; }
        let mut val: i32 = 0;
        extern "C" { fn CFNumberGetValue(n: *const c_void, t: u32, v: *mut c_void) -> u8; }
        CFNumberGetValue(v, 3, &mut val as *mut i32 as *mut c_void);
        val
    }
    unsafe fn get_str(dev: *const c_void, key: &str) -> String {
        let k = cfstr(key);
        let v = IOHIDDeviceGetProperty(dev, k);
        CFRelease(k);
        if v.is_null() { return String::new(); }
        let mut buf = [0i8; 256];
        CFStringGetCString(v, buf.as_mut_ptr(), 256, 0x08000100);
        let bytes: Vec<u8> = buf.iter().map(|&b| b as u8).take_while(|&b| b != 0).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    QUEUE.set(Arc::new(Mutex::new(VecDeque::new()))).unwrap();

    unsafe {
        // Match: VendorID=0x046D + ProductID=0xB035 + Transport=BLE
        // Exactly like Python's open()
        let k1 = cfstr("VendorID");
        let v1 = cfnum(0x046D);
        let k2 = cfstr("ProductID");
        let v2 = cfnum(0xB035);
        let k3 = cfstr("Transport");
        let v3 = cfstr("Bluetooth Low Energy");

        let keys = [k1, k2, k3];
        let vals = [v1, v2, v3];

        let dict = CFDictionaryCreate(ptr::null(), keys.as_ptr(), vals.as_ptr(), 3, ptr::null(), ptr::null());

        let mgr = IOHIDManagerCreate(ptr::null(), 0);
        IOHIDManagerSetDeviceMatching(mgr, dict);
        let ret = IOHIDManagerOpen(mgr, 0);
        println!("IOHIDManagerOpen: {ret}");

        let devices = IOHIDManagerCopyDevices(mgr);
        if devices.is_null() {
            println!("No devices found!");
            return;
        }
        let count = CFSetGetCount(devices);
        println!("Found {count} devices");

        let mut devs = vec![ptr::null(); count as usize];
        CFSetGetValues(devices, devs.as_mut_ptr());
        CFRelease(devices);

        let dev = devs[0];
        let pid = get_int(dev, "ProductID") as u16;
        let up = get_int(dev, "PrimaryUsagePage") as u16;
        let usage = get_int(dev, "PrimaryUsage") as u16;
        let product = get_str(dev, "Product");
        println!("Device: PID=0x{pid:04X} UP=0x{up:04X} usage=0x{usage:04X} product={product}");

        CFRetain(dev);

        let ret = IOHIDDeviceOpen(dev, 0);
        println!("IOHIDDeviceOpen: {ret}");

        let rl = CFRunLoopGetCurrent();

        // Allocate input buffer
        let input_buf = std::alloc::alloc_zeroed(std::alloc::Layout::from_size_align(64, 1).unwrap());

        IOHIDDeviceScheduleWithRunLoop(dev, rl, kCFRunLoopDefaultMode);
        IOHIDDeviceRegisterInputReportCallback(dev, input_buf, 64, report_cb, ptr::null_mut());

        // Send IRoot request: [0x11, 0xFF, 0x00, 0x0A, 0x1B, 0x04, ...] (find REPROG_V4)
        let mut buf = [0u8; 20];
        buf[0] = 0x11; // LONG_REPORT_ID
        buf[1] = 0xFF; // dev_idx BT direct
        buf[2] = 0x00; // IRoot feature index
        buf[3] = 0x0A; // func=0, sw=0x0A
        buf[4] = 0x1B; // REPROG_V4 feature ID high
        buf[5] = 0x04; // REPROG_V4 feature ID low

        println!("Sending IRoot request: [{:02X?}]", &buf[..8]);
        let ret = IOHIDDeviceSetReport(dev, 1, 0x11, buf.as_ptr(), 20);
        println!("IOHIDDeviceSetReport: {ret}");

        // Read response by pumping run loop
        println!("Waiting for response...");
        for _ in 0..40 {  // 40 * 50ms = 2 seconds
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, 0);
            let q = QUEUE.get().unwrap();
            let mut g = q.lock().unwrap();
            while let Some(report) = g.pop_front() {
                let hex: Vec<String> = report.iter().map(|b| format!("{b:02X}")).collect();
                println!("  Received report ({} bytes): [{}]", report.len(), hex.join(" "));

                // Check if this is an HID++ response (report_id 0x11)
                if report.len() >= 5 && (report[0] == 0x10 || report[0] == 0x11) {
                    let feat_idx = report[2];
                    let func_sw = report[3];
                    if report[2] == 0xFF {
                        // Error response
                        let error_code = report.get(5).copied().unwrap_or(0);
                        println!("  → HID++ ERROR: code=0x{error_code:02X}");
                    } else {
                        println!("  → HID++ response: feat_idx=0x{feat_idx:02X} func_sw=0x{func_sw:02X}");
                        if report.len() > 4 {
                            println!("  → REPROG_V4 feature index = 0x{:02X}", report[4]);
                        }
                    }
                }
            }
        }

        // Cleanup (keep mgr alive until here!)
        IOHIDDeviceClose(dev, 0);
        CFRelease(dev);
        // Don't release manager yet - keep alive
        for &k in &keys { CFRelease(k); }
        for &v in &vals { CFRelease(v); }
        CFRelease(dict);
        CFRelease(mgr as *const c_void);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("This test only runs on macOS");
}
