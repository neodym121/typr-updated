//! Whether this computer can run models on the GPU, found without loading
//! the runtime: the display adapters from the registry plus the Vulkan
//! loader (vulkan-1.dll) that GPU drivers install.

use serde::Serialize;

use super::catalog::Backend;

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub name: String,
    /// Dedicated memory in bytes, 0 when the driver doesn't report it
    pub memory: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Hardware {
    /// Most dedicated memory first, so a discrete GPU comes before an
    /// integrated one
    pub gpus: Vec<Gpu>,
    /// vulkan-1.dll: a Vulkan loader is installed
    pub vulkan_loader: bool,
}

impl Hardware {
    pub fn detect() -> Hardware {
        let mut hardware = detect_platform();
        hardware.gpus.sort_by(|a, b| b.memory.cmp(&a.memory));
        log::info!(
            "GPUs: {}; Vulkan loader: {}",
            if hardware.gpus.is_empty() {
                "none".to_string()
            } else {
                hardware
                    .gpus
                    .iter()
                    .map(|g| format!("{} ({} MB)", g.name, g.memory / (1024 * 1024)))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            hardware.vulkan_loader
        );
        hardware
    }

    pub fn is_available(&self, backend: Backend) -> bool {
        match backend {
            Backend::Vulkan => self.vulkan_loader && !self.gpus.is_empty(),
            Backend::Cpu => true,
        }
    }

    /// Vulkan when there is a GPU to run it, else the CPU.
    pub fn recommended(&self) -> Backend {
        if self.is_available(Backend::Vulkan) {
            Backend::Vulkan
        } else {
            Backend::Cpu
        }
    }

    /// The backend for a saved choice: empty (automatic) or one this
    /// computer can't use falls back to the recommended one.
    pub fn resolve(&self, choice: &str) -> Backend {
        Backend::parse(choice)
            .filter(|b| self.is_available(*b))
            .unwrap_or_else(|| self.recommended())
    }
}

/// A real adapter's hardware id, such as `pci\ven_1002&dev_7480&...`
/// (virtual and basic display drivers have none)
fn is_pci_device(hardware_id: &str) -> bool {
    hardware_id.to_ascii_lowercase().starts_with("pci\\ven_")
}

#[cfg(windows)]
fn detect_platform() -> Hardware {
    let system = std::env::var_os("SystemRoot")
        .map(|root| std::path::PathBuf::from(root).join("System32"))
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows\System32"));
    Hardware {
        gpus: registry::display_adapters(),
        vulkan_loader: system.join("vulkan-1.dll").is_file(),
    }
}

#[cfg(not(windows))]
fn detect_platform() -> Hardware {
    Hardware::default()
}

#[cfg(windows)]
mod registry {
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE,
        KEY_READ, RRF_RT_DWORD, RRF_RT_REG_QWORD, RRF_RT_REG_SZ,
    };

    use super::{is_pci_device, Gpu};

    /// Every installed display adapter has a numbered subkey here
    const DISPLAY_CLASS: &str =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn read_string(key: HKEY, subkey: &[u16], value: &str) -> Option<String> {
        let value = wide(value);
        let mut buffer = [0u16; 512];
        let mut size = (buffer.len() * 2) as u32;
        let status = RegGetValueW(
            key,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr() as *mut c_void,
            &mut size,
        );
        if status != ERROR_SUCCESS {
            return None;
        }
        let chars = (size as usize / 2).min(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..chars]);
        Some(text.trim_end_matches('\0').trim().to_string())
    }

    unsafe fn read_memory(key: HKEY, subkey: &[u16]) -> u64 {
        let qword = wide("HardwareInformation.qwMemorySize");
        let mut value: u64 = 0;
        let mut size = 8u32;
        if RegGetValueW(
            key,
            subkey.as_ptr(),
            qword.as_ptr(),
            RRF_RT_REG_QWORD,
            null_mut(),
            &mut value as *mut u64 as *mut c_void,
            &mut size,
        ) == ERROR_SUCCESS
        {
            return value;
        }
        // Older drivers: a DWORD (or 4-byte binary) value
        let dword = wide("HardwareInformation.MemorySize");
        let mut value: u32 = 0;
        let mut size = 4u32;
        if RegGetValueW(
            key,
            subkey.as_ptr(),
            dword.as_ptr(),
            RRF_RT_DWORD,
            null_mut(),
            &mut value as *mut u32 as *mut c_void,
            &mut size,
        ) == ERROR_SUCCESS
        {
            return value as u64;
        }
        0
    }

    pub fn display_adapters() -> Vec<Gpu> {
        let mut gpus = Vec::new();
        unsafe {
            let mut class: HKEY = null_mut();
            let path = wide(DISPLAY_CLASS);
            if RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_READ, &mut class)
                != ERROR_SUCCESS
            {
                log::warn!("Could not open the display adapter list in the registry");
                return gpus;
            }

            for index in 0u32..64 {
                let mut name = [0u16; 256];
                let mut length = name.len() as u32;
                let status = RegEnumKeyExW(
                    class,
                    index,
                    name.as_mut_ptr(),
                    &mut length,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                );
                if status != ERROR_SUCCESS {
                    break;
                }
                let subkey: Vec<u16> = name[..length as usize]
                    .iter()
                    .copied()
                    .chain(std::iter::once(0))
                    .collect();

                let is_pci = read_string(class, &subkey, "MatchingDeviceId")
                    .map_or(false, |id| is_pci_device(&id));
                if !is_pci {
                    continue;
                }
                let Some(name) = read_string(class, &subkey, "DriverDesc") else {
                    continue;
                };
                gpus.push(Gpu {
                    name,
                    memory: read_memory(class, &subkey),
                });
            }
            RegCloseKey(class);
        }
        gpus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu() -> Gpu {
        Gpu {
            name: "GPU".to_string(),
            memory: 8 << 30,
        }
    }

    #[test]
    fn test_is_pci_device() {
        assert!(is_pci_device(r"PCI\VEN_10DE&DEV_2684"));
        assert!(is_pci_device(r"pci\ven_1002&dev_7480&subsys_0"));
        assert!(!is_pci_device(r"ROOT\BasicDisplay"));
        assert!(!is_pci_device(""));
    }

    #[test]
    fn test_gpu_with_vulkan_prefers_vulkan() {
        let hw = Hardware {
            gpus: vec![gpu()],
            vulkan_loader: true,
        };
        assert_eq!(hw.recommended(), Backend::Vulkan);
        assert_eq!(hw.resolve(""), Backend::Vulkan);
        assert_eq!(hw.resolve("cpu"), Backend::Cpu);
        // Choices of earlier builds fall back to the recommended one
        assert_eq!(hw.resolve("rocm"), Backend::Vulkan);
    }

    #[test]
    fn test_no_gpu_or_no_vulkan_gets_the_cpu() {
        assert_eq!(Hardware::default().recommended(), Backend::Cpu);
        assert!(!Hardware::default().is_available(Backend::Vulkan));
        let no_driver = Hardware {
            gpus: vec![gpu()],
            vulkan_loader: false,
        };
        assert_eq!(no_driver.resolve("vulkan"), Backend::Cpu);
        assert!(no_driver.is_available(Backend::Cpu));
    }
}
