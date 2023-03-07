use esp_idf_sys::{
    esp, nvs_flash_deinit, nvs_flash_erase, nvs_flash_init, ESP_ERR_NVS_NEW_VERSION_FOUND,
    ESP_ERR_NVS_NO_FREE_PAGES,
};

use anyhow::{bail, Result};

pub struct Esp32 {
    // TODO: mutex around this?
    nvs_initialized_count: usize,
}

impl Esp32 {
    pub fn new() -> Self {
        Esp32 {
            nvs_initialized_count: 0,
        }
    }

    pub fn init(&self) {
        esp_idf_sys::link_patches();

        esp_idf_svc::log::EspLogger::initialize_default();
    }

    pub fn nvs_init(&mut self) -> Result<()> {
        if self.nvs_initialized_count == 0 {
            let result = unsafe { esp!(nvs_flash_init()) };

            if let Err(e) = result {
                match e.code() {
                    ESP_ERR_NVS_NO_FREE_PAGES | ESP_ERR_NVS_NEW_VERSION_FOUND => unsafe {
                        esp!(nvs_flash_erase())?;
                        esp!(nvs_flash_init())?;
                    },
                    _ => bail!(e),
                };
            }
        }
        self.nvs_initialized_count += 1;
        Ok(())
    }

    pub fn nvs_deinit(&mut self) -> Result<()> {
        if self.nvs_initialized_count == 0 {
            bail!("Not initialized")
        } else {
            self.nvs_initialized_count -= 1;

            if self.nvs_initialized_count == 0 {
                unsafe { esp!(nvs_flash_deinit())? }
            }
            Ok(())
        }
    }
}
