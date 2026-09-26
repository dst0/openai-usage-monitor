use super::codex_process_probe::codex_app_pids_checked;
use crate::distribution::{
    SystemWindowRestoreBackend, WindowProcessIdentity, WindowProcessValidationService,
};
use std::{
    ffi::{c_char, c_void, CString},
    path::Path,
    ptr,
};

const APP_PATH: &str = "/Applications/ChatGPT.app";
const APP_URL: &str = "file:///Applications/ChatGPT.app/";
const UTF8: u32 = 0x0800_0100;
const LAUNCH_DEFAULTS: u32 = 0x0000_0001;
const LAUNCH_DONT_SWITCH: u32 = 0x0000_0200;
const IDENTITY_CHANGED: &str = "ChatGPT process identity changed during task navigation";

pub(crate) fn is_identity_change(error: &str) -> bool {
    error == IDENTITY_CHANGED
}

// Carbon's LSLaunchURLSpec has two-byte packing in the macOS SDK. Its
// layout is checked by the size assertion before calling LaunchServices.
#[repr(C, packed(2))]
struct LaunchSpec {
    app_url: *const c_void,
    item_urls: *const c_void,
    pass_thru_params: *const c_void,
    launch_flags: u32,
    async_ref_con: *mut c_void,
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeArrayCallBacks: u8;
    fn CFStringCreateWithCString(
        allocator: *const c_void,
        text: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFURLCreateWithString(
        allocator: *const c_void,
        text: *const c_void,
        base: *const c_void,
    ) -> *const c_void;
    fn CFArrayCreate(
        allocator: *const c_void,
        values: *const *const c_void,
        count: isize,
        callbacks: *const c_void,
    ) -> *const c_void;
    fn CFRelease(value: *const c_void);
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSOpenFromURLSpec(spec: *const LaunchSpec, launched: *mut *const c_void) -> i32;
}

fn exact_desktop_identity() -> Result<WindowProcessIdentity, String> {
    let pids = codex_app_pids_checked()?;
    if pids.len() != 1 {
        return Err("Native task navigation requires one ChatGPT main process".into());
    }
    let mut backend = SystemWindowRestoreBackend::new()?;
    WindowProcessValidationService::inspect(&mut backend, pids[0])
}

fn create_url(url: &str) -> Result<*const c_void, String> {
    let c_url = CString::new(url).map_err(|_| "Invalid ChatGPT task URL")?;
    // SAFETY: CoreFoundation copies the NUL-terminated UTF-8 bytes. The
    // returned references are released by the caller on all paths.
    unsafe {
        let text = CFStringCreateWithCString(ptr::null(), c_url.as_ptr(), UTF8);
        if text.is_null() {
            return Err("Could not create ChatGPT task URL".into());
        }
        let url = CFURLCreateWithString(ptr::null(), text, ptr::null());
        CFRelease(text);
        if url.is_null() {
            return Err("Could not create ChatGPT task URL".into());
        }
        Ok(url)
    }
}

fn launch_pinned_link(thread_id: &str) -> Result<(), String> {
    if std::mem::size_of::<LaunchSpec>() != 36 {
        return Err("Unsupported LaunchServices URL layout".into());
    }
    let app = create_url(APP_URL)?;
    let item = match create_url(&format!("codex://threads/{thread_id}")) {
        Ok(item) => item,
        Err(error) => {
            // SAFETY: app is an owned non-null CoreFoundation reference.
            unsafe { CFRelease(app) };
            return Err(error);
        }
    };
    let values = [item];
    // SAFETY: All input references remain live through LSOpenFromURLSpec.
    // kCFTypeArrayCallBacks makes the array retain its item URL if
    // LaunchServices processes the launch asynchronously.
    let status = unsafe {
        let items = CFArrayCreate(
            ptr::null(),
            values.as_ptr(),
            1,
            ptr::addr_of!(kCFTypeArrayCallBacks).cast(),
        );
        if items.is_null() {
            CFRelease(item);
            CFRelease(app);
            return Err("Could not create ChatGPT task URL array".into());
        }
        let spec = LaunchSpec {
            app_url: app,
            item_urls: items,
            pass_thru_params: ptr::null(),
            launch_flags: LAUNCH_DEFAULTS | LAUNCH_DONT_SWITCH,
            async_ref_con: ptr::null_mut(),
        };
        let mut launched = ptr::null();
        let status = LSOpenFromURLSpec(&spec, &mut launched);
        if !launched.is_null() {
            CFRelease(launched);
        }
        CFRelease(items);
        CFRelease(item);
        CFRelease(app);
        status
    };
    if status != 0 {
        return Err(format!(
            "Pinned ChatGPT task navigation failed (OSStatus {status})"
        ));
    }
    Ok(())
}

pub(super) fn retry(thread_id: &str) -> Result<(), String> {
    #[cfg(test)]
    crate::test_live_system::forbid("pinned ChatGPT task link (LaunchServices)");
    // Callers validate too; this keeps an invalid ID away from LaunchServices
    // and makes the tripwire's guard test inert if the tripwire is removed.
    if !super::thread_identity::is_valid_thread_id(thread_id) {
        return Err("Invalid thread ID for ChatGPT navigation".into());
    }
    let metadata =
        std::fs::symlink_metadata(APP_PATH).map_err(|_| "The pinned ChatGPT app is unavailable")?;
    if !metadata.file_type().is_dir() || Path::new(APP_PATH).is_symlink() {
        return Err("The pinned ChatGPT app path is not a directory".into());
    }
    let before = exact_desktop_identity()?;
    let delivery = launch_pinned_link(thread_id);
    let after = exact_desktop_identity().map_err(|_| IDENTITY_CHANGED.to_string())?;
    if after != before {
        return Err(IDENTITY_CHANGED.into());
    }
    delivery
}

#[cfg(test)]
#[path = "pinned_thread_link_launch_spec.test.rs"]
mod tests;
