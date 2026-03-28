#![allow(dead_code)]
// accessibility.rs — macOS Accessibility API trust helpers.
//
// On macOS the process must be granted Accessibility permission before it can
// observe global input events.  These helpers wrap the two relevant symbols
// from the ApplicationServices framework:
//
//   AXIsProcessTrusted()            — query without prompting
//   AXIsProcessTrustedWithOptions() — query and optionally show the system prompt
//
// On all other platforms both functions always return `true` (permission is
// considered unconditionally granted).

// ---------------------------------------------------------------------------
// macOS — link to ApplicationServices framework
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod macos_impl {
    use std::ffi::c_void;

    // External symbols from ApplicationServices.framework
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }

    // External symbols from CoreFoundation.framework (for building the options dict)
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        // kAXTrustedCheckOptionPrompt — CFStringRef
        static kAXTrustedCheckOptionPrompt: *const c_void;
        // kCFBooleanTrue — CFBooleanRef
        static kCFBooleanTrue: *const c_void;

        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;

        fn CFRelease(cf: *const c_void);
    }

    /// Check whether the current process is trusted for Accessibility access.
    ///
    /// When `prompt` is `true` and the process is *not* trusted, macOS shows
    /// the standard "Allow in System Settings > Privacy & Security" sheet.
    pub fn is_process_trusted(prompt: bool) -> bool {
        unsafe {
            if prompt {
                // Build { kAXTrustedCheckOptionPrompt: kCFBooleanTrue }
                let keys: [*const c_void; 1] = [kAXTrustedCheckOptionPrompt];
                let values: [*const c_void; 1] = [kCFBooleanTrue];
                let dict = CFDictionaryCreate(
                    std::ptr::null(),
                    keys.as_ptr(),
                    values.as_ptr(),
                    1,
                    std::ptr::null(),
                    std::ptr::null(),
                );
                if !dict.is_null() {
                    let trusted = AXIsProcessTrustedWithOptions(dict);
                    CFRelease(dict);
                    return trusted;
                }
                // Fall through to no-prompt version if dict creation failed
            }
            AXIsProcessTrusted()
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns `true` if the process has been granted Accessibility permission.
///
/// On platforms other than macOS this always returns `true`.
pub fn is_process_trusted() -> bool {
    #[cfg(target_os = "macos")]
    { macos_impl::is_process_trusted(false) }

    #[cfg(not(target_os = "macos"))]
    { true }
}

/// Check Accessibility trust and, if not already trusted, show the system
/// permission prompt on macOS.
///
/// Returns `true` if the process is trusted (either before or after the
/// prompt).  On non-macOS platforms always returns `true`.
pub fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    { macos_impl::is_process_trusted(true) }

    #[cfg(not(target_os = "macos"))]
    { true }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_process_trusted_non_macos() {
        // On non-macOS platforms the function must always return true.
        #[cfg(not(target_os = "macos"))]
        assert!(is_process_trusted());
    }

    #[test]
    fn test_request_accessibility_non_macos() {
        #[cfg(not(target_os = "macos"))]
        assert!(request_accessibility());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_is_process_trusted_macos_does_not_panic() {
        // In a CI / test context without Accessibility permission the function
        // should return false without panicking.
        let _ = is_process_trusted();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_request_accessibility_macos_does_not_panic() {
        // NOTE: this will NOT show a prompt in headless CI because
        // AXIsProcessTrustedWithOptions only shows the dialog in interactive
        // GUI sessions.  It is safe to call here.
        let _ = request_accessibility();
    }
}
