// use std::{fs::File, io::Write};

use esp_idf_sys::{
    esp_vfs_fat_sdspi_mount, gpio_num_t_GPIO_NUM_NC, sdmmc_host_t, sdspi_device_config_t,
    sdspi_host_do_transaction, sdspi_host_init, sdspi_host_io_int_enable, sdspi_host_io_int_wait,
    sdspi_host_set_card_clk, spi_bus_config_t, spi_bus_config_t__bindgen_ty_1,
    spi_bus_config_t__bindgen_ty_2, spi_bus_config_t__bindgen_ty_3, spi_bus_config_t__bindgen_ty_4,
    spi_bus_initialize, spi_common_dma_t_SPI_DMA_CH_AUTO, spi_host_device_t_SPI3_HOST,
    SPICOMMON_BUSFLAG_MASTER,
};

use anyhow::Result;

/* On our Lolin ESP32 Pro boards, CS pin is GPIO 4 */
/* MOSI: IO23
SCK: IO18
MISO: IO19
*/
pub fn init(pin_cs: i32, pin_sclk: i32, pin_miso: i32, pin_mosi: i32) -> Result<()> {
    const SDMMC_FREQ_DEFAULT: i32 = 20000;

    log::info!("init_sd_card: entry");

    // Using what the macro defined here does: https://github.com/espressif/esp-idf/blob/4778c249e64e449db0e22b35b9dcf1abddaf2503/components/driver/include/driver/sdspi_host.h#L38
    let host_config = sdmmc_host_t {
        flags: (1 << 3) /* SDMMC_HOST_FLAG_SPI */ | (1 << 5), /* SDMMC_HOST_FLAG_DEINIT_ARG*/
        slot: 0, /* slot 0 is 8 bit wide, maps to HS1_* signals in PIN MUX */
        max_freq_khz: SDMMC_FREQ_DEFAULT,
        io_voltage: 3.3,
        init: Some(sdspi_host_init),
        set_bus_width: None,    /* NULL */
        get_bus_width: None,    /* NULL */
        set_bus_ddr_mode: None, /* NULL */
        set_card_clk: Some(sdspi_host_set_card_clk),
        do_transaction: Some(sdspi_host_do_transaction),
        /* Not sure why this doesn't work: */ /*deinit_p: Some(sdspi_host_remove_device), */
        io_int_enable: Some(sdspi_host_io_int_enable),
        io_int_wait: Some(sdspi_host_io_int_wait),
        command_timeout_ms: 0,

        ..Default::default()
    };

    let spi_config = sdspi_device_config_t {
        host_id: spi_host_device_t_SPI3_HOST, /* VSPI = SPI3 */
        gpio_cs: pin_cs,
        gpio_cd: gpio_num_t_GPIO_NUM_NC,  /* CD = Card Detect */
        gpio_wp: gpio_num_t_GPIO_NUM_NC,  /* WP = Write Protect */
        gpio_int: gpio_num_t_GPIO_NUM_NC, /* int = Interrupt */
    };

    let vfs_config = esp_idf_sys::esp_vfs_fat_mount_config_t {
        format_if_mount_failed: true,
        max_files: 8,
        allocation_unit_size: 0,
    };

    log::info!("init_sd_card: before let spi_bus_config");

    let spi_bus_config = spi_bus_config_t {
        sclk_io_num: pin_sclk,
        max_transfer_sz: 4096,
        flags: SPICOMMON_BUSFLAG_MASTER,
        intr_flags: 0,
        /* Use the below when building without features native */
        /*
        mosi_io_num: pin_mosi,
        miso_io_num: pin_miso,
        quadwp_io_num: -1,
        quadhd_io_num: -1,
         */
        /* if building with features native, use the below: */
        data4_io_num: -1,
        data5_io_num: -1,
        data6_io_num: -1,
        data7_io_num: -1,

        __bindgen_anon_1: spi_bus_config_t__bindgen_ty_1 {
            mosi_io_num: pin_mosi,
            //data0_io_num: -1,
        },
        __bindgen_anon_2: spi_bus_config_t__bindgen_ty_2 {
            miso_io_num: pin_miso,
            //data1_io_num: -1,
        },
        __bindgen_anon_3: spi_bus_config_t__bindgen_ty_3 {
            quadwp_io_num: -1,
            //data2_io_num: -1,
        },
        __bindgen_anon_4: spi_bus_config_t__bindgen_ty_4 {
            quadhd_io_num: -1,
            //data3_io_num: -1,
        },
        // ..Default::default()
    };

    // let card: &mut sdmmc_card_t;

    log::info!("Before SD Mount");
    let sd_card = std::ffi::CString::new("/sdcard")?;
    unsafe {
        esp_idf_sys::esp!(spi_bus_initialize(
            spi_host_device_t_SPI3_HOST,
            &spi_bus_config,
            spi_common_dma_t_SPI_DMA_CH_AUTO
        ))?;

        esp_idf_sys::esp!(esp_vfs_fat_sdspi_mount(
            sd_card.as_ptr(),
            &host_config,
            &spi_config,
            &vfs_config,
            std::ptr::null_mut()
        ))? // TODO: Should we uninitialize SPI bus on error here?
    }

    log::info!("SD Card mounted");
    /*
       let mut file = File::create("/sdcard/foo.txt")?;
       file.write_all(b"Hello, world X!")?;

       log::info!("SD Card file written");
    */
    Ok(())
}
