//! NVS key-value storage, through ESP-IDF's C API. The namespaces and keys
//! are the C++ firmware's ("bdlminer"/cfgA, "stats"/statA...), and NVS
//! stays at 0x9000: an update from it keeps settings and statistics.

use std::ffi::CString;

use bdlminer_app::platform::Storage;
use esp_idf_svc::sys::*;

pub struct NvsStorage;

struct Handle(nvs_handle_t);

impl Handle {
    fn open(namespace: &str, write: bool) -> Option<Handle> {
        let name = CString::new(namespace).ok()?;
        let mode = if write { nvs_open_mode_t_NVS_READWRITE } else { nvs_open_mode_t_NVS_READONLY };
        let mut handle: nvs_handle_t = 0;
        (unsafe { nvs_open(name.as_ptr(), mode, &mut handle) } == ESP_OK).then_some(Handle(handle))
    }
    fn commit(&self) -> bool {
        unsafe { nvs_commit(self.0) == ESP_OK }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { nvs_close(self.0) };
    }
}

impl Storage for NvsStorage {
    fn get(&mut self, namespace: &str, key: &str) -> Option<Vec<u8>> {
        let h = Handle::open(namespace, false)?;
        let key = CString::new(key).ok()?;
        let mut length: usize = 0;
        if unsafe { nvs_get_blob(h.0, key.as_ptr(), std::ptr::null_mut(), &mut length) } != ESP_OK {
            return None;
        }
        let mut data = vec![0u8; length];
        let ok = unsafe { nvs_get_blob(h.0, key.as_ptr(), data.as_mut_ptr().cast(), &mut length) } == ESP_OK;
        (ok && length == data.len()).then_some(data)
    }

    fn set(&mut self, namespace: &str, key: &str, value: &[u8]) -> bool {
        let (Some(h), Ok(key)) = (Handle::open(namespace, true), CString::new(key)) else { return false };
        let ok = unsafe { nvs_set_blob(h.0, key.as_ptr(), value.as_ptr().cast(), value.len()) } == ESP_OK;
        ok && h.commit()
    }

    fn remove(&mut self, namespace: &str, key: &str) -> bool {
        let (Some(h), Ok(key)) = (Handle::open(namespace, true), CString::new(key)) else { return false };
        let ok = unsafe { nvs_erase_key(h.0, key.as_ptr()) } == ESP_OK;
        ok && h.commit()
    }

    fn clear(&mut self, namespace: &str) -> bool {
        let Some(h) = Handle::open(namespace, true) else { return false };
        let ok = unsafe { nvs_erase_all(h.0) } == ESP_OK;
        ok && h.commit()
    }
}
