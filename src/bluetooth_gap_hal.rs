use core::fmt;
use std::fmt::Debug;
use std::fmt::Display;

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use anyhow::Result;

use crate::bluetooth_hal::{bdaddr_to_string, BDAddr};

// See https://btprodspecificationrefs.blob.core.windows.net/assigned-numbers/Assigned%20Number%20Types/Assigned%20Numbers.pdf
// section 2.
#[repr(u8)]
#[derive(Debug, FromPrimitive)]
pub enum CommonDataType {
    Flags = 0x01,
    Incomplete16bitServiceClassUUIDs = 0x02,
    Complete16bitServiceClassUUIDs = 0x03,
    Incomplete32bitServiceClassUUIDs = 0x04,
    Complete32bitServiceClassUUIDs = 0x05,
    Incomplete128bitServiceClassUUIDs = 0x06,
    Complete128bitServiceClassUUIDs = 0x07,
    ShortenedLocalName = 0x08,
    CompleteLocalName = 0x09,
    TxPowerLevel = 0x0a,
    ClassOfDevice = 0x0d,
    SimplePairingHashC192 = 0x0e,
    SimplePairingRandomizerR192 = 0x0f,
    DeviceID = 0x10, // or Security Manager TK Value ?
    SecurityManagerOutOfBandFlags = 0x11,
    PeripheralConnectionIntervalRange = 0x12,
    ListOf16BitServiceSolicitation = 0x14,
    ListOf128BitServiceSolicitation = 0x15,
    ManufacturerSpecificData = 0xff,
}

#[derive(Debug, FromPrimitive)]
pub enum MajorClass {
    Miscellaneous = 0,
    Computer = 1,
    Phone = 2,
    Network = 3,
    AudioVideo = 4,
    Peripheral = 5,
    Imaging = 6,
    Wearable = 7,
    Toy = 8,
    Health = 9,
    Uncategorized = 0x1f,
}

pub struct ClassOfDevice {
    pub value: u32,
}

impl ClassOfDevice {
    fn get_minor_device_class(&self) -> u8 {
        (self.value & 0xf6) as u8 >> 2
    }
    fn get_major_device_class(&self) -> Result<MajorClass> {
        let value = (self.value >> 8) as u8 & 0x1f;
        let opt_class: Option<MajorClass> = FromPrimitive::from_u8(value);

        match opt_class {
            Some(mc) => Ok(mc),
            // None => Err(std::error::Error::new((format!("Invalid major {}", value)))),
            None => Err(anyhow::anyhow!(format!("Invalid major {}", value))),
        }
    }

    fn get_service_classes(&self) -> u16 {
        (self.value >> 13) as u16 & 0x7ff
    }
    /*
       fn major_device_class_to_string(class: u8) -> &'static str {
           match class {
               0 => "Miscellaneous",
               1 => "Computer",
               2 => "Phone",
               3 => "LAN /Network Access point",
               4 => "Audio/Video",
               5 => "Peripheral",
               6 => "Imaging",
               7 => "Wearable",
               8 => "Toy",
               9 => "Health",
               0x1f => "Uncategorized",
               _ => "Invalid",
           }
       }
    */
    fn minor_device_class_to_string(major: u8, minor: u8) -> String {
        match major {
            2 => {
                // Phone
                match minor {
                    1 => "Cellular".to_string(),
                    _ => format!("Phone major, minor {} NIY", minor),
                }
            }
            4 => {
                // Audio/Video
                match minor {
                    1 => "Wearable Headset Device".to_string(),
                    _ => format!("Audio major, minor {} NIY", minor),
                }
            }
            _ => format!("Unimplemented major {}", major),
        }
        /*        match class {
                   0 => "Uncategorized",
                   1 => "Desktop Workstation",
                   2 => "Server-class computer",
                   3 => "Laptop",
        */
        // format!("Minor: 0x{:02x}-0x{:02x}", major, minor)
    }

    fn service_classes_to_string(service: u16) -> String {
        const BITS: [(u8, &str); 9] = [
            (0, "Limited Discoverable Mode"),
            (3, "Positioning"),
            (4, "Networking"),
            (5, "Rendering"),
            (6, "Capturing"),
            (7, "Object Transfer"),
            (8, "Audio"),
            (9, "Telephony"),
            (10, "Information"),
        ];

        let mut str = String::new();
        let mut first = true;

        for bit in BITS {
            if service & (1 << bit.0) != 0 {
                str.push_str(bit.1);
                if first {
                    first = false
                } else {
                    str.push_str(", ");
                }
            }
        }
        str
    }
}

impl Display for ClassOfDevice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let major = self.get_major_device_class();
        write!(
            f,
            "Major: {:?}",
            major // ClassOfDevice::major_device_class_to_string(major)
        )?;
        write!(
            f,
            ", Minor: {}",
            ClassOfDevice::minor_device_class_to_string(
                self.get_major_device_class().unwrap() as u8,
                self.get_minor_device_class()
            )
        )?;
        write!(
            f,
            ", Service Classes: {}",
            ClassOfDevice::service_classes_to_string(self.get_service_classes())
        )
    }
}

impl Debug for ClassOfDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
        // f.debug_struct("ClassOfDevice").field("value", &self.value).finish()
    }
}

#[derive(Debug)]
pub enum DeviceProperty {
    Name(String),
    Class(ClassOfDevice),
    Rssi(i8),
    Eir(Vec<u8>),
}

#[derive(Default)]
pub struct ScannedDevice {
    pub address: BDAddr,
    pub class: Option<ClassOfDevice>,
    pub name: Option<String>,
    pub rssi: Option<i8>,
    pub complete_16_bit_service_class_uuids: Option<Vec<u16>>,
    pub complete_32_bit_service_class_uuids: Option<Vec<u32>>,
}

impl ScannedDevice {
    pub fn add_property(&mut self, property: DeviceProperty) {
        match property {
            DeviceProperty::Name(name) => self.name = Some(name),
            DeviceProperty::Class(class) => self.class = Some(class),
            DeviceProperty::Rssi(rssi) => self.rssi = Some(rssi),
            DeviceProperty::Eir(eir) => self.parse_eir(eir),
        }
    }

    fn parse_eir(&mut self, eir: Vec<u8>) {
        let mut index = 0;
        while index < eir.len() {
            let len: usize = eir[index] as usize;

            if len == 0 {
                break;
            }
            index += 1;
            let t_val = eir[index] as u8;
            let t: Option<CommonDataType> = FromPrimitive::from_u8(eir[index] as u8);
            let record_start = index;

            if let Some(typ) = t {
                index += 1;
                match typ {
                    CommonDataType::Complete16bitServiceClassUUIDs => {
                        let mut uuids = Vec::new();

                        while index < (record_start + len) {
                            let uuid = (eir[index] as u16) | (eir[index + 1] as u16) << 8;
                            index += 2;
                            uuids.push(uuid);
                        }
                        // log::info!("uuids={:?}", uuids);
                        self.complete_16_bit_service_class_uuids = Some(uuids);
                    }
                    CommonDataType::Complete32bitServiceClassUUIDs => {
                        let mut uuids = Vec::new();

                        while index < (record_start + len) {
                            let uuid = (eir[index] as u32)
                                | (eir[index + 1] as u32) << 8
                                | (eir[index + 2] as u32) << 16
                                | (eir[index + 3] as u32) << 24;
                            index += 4;
                            uuids.push(uuid);
                        }
                        // log::info!("uuids={:?}", uuids);
                        self.complete_32_bit_service_class_uuids = Some(uuids);
                    }
                    CommonDataType::CompleteLocalName => {
                        // log::info!("Name len={}", len);
                        let s = String::from_utf8(eir[index..(index + len - 1)].to_vec());
                        match s {
                            Ok(s) => {
                                log::info!("name={}", s);
                                self.name = Some(s)
                            }
                            Err(e) => log::error!("Failed to parse CompleteLocalName: {}", e),
                        }
                        index += len - 1;
                    }
                    _ => {
                        log::warn!("Parsing eir, got type: {:?}", typ);
                        index += len - 1
                    }
                } // end of match
            }
            // end of if let
            else {
                log::warn!("Parsing eir, got type: {:?}", t_val);
                index += len;
            }
        } // end of while else
    }

    pub fn has_16bit_uuid(&self, uuid: u16) -> bool {
        if let Some(uuids) = &self.complete_16_bit_service_class_uuids {
            // log::info!("uuids={:?}, uuid={}", uuids, uuid);
            uuids.contains(&uuid)
        } else {
            log::info!("Device doesn't have 16 bit uuids");
            false
        }
    }
    /*
    pub fn get_name(&self) -> &str {
        &self.name.unwrap()
    }
     */
}

impl Debug for ScannedDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Address: {} ", bdaddr_to_string(self.address))?;

        if let Some(class) = &self.class {
            write!(f, "Class: {} ", class)?;
        }

        if let Some(name) = &self.name {
            write!(f, "Name: {} ", name)?;
        }

        if let Some(uuids) = &self.complete_16_bit_service_class_uuids {
            write!(f, "Complete 16 bit UUIDS: [")?;
            for uuid in uuids {
                write!(f, "{:04x} ", uuid)?;
            }
            write!(f, "] ")?;
        }

        if let Some(rssi) = &self.rssi {
            write!(f, "RSSI: {}", *rssi)
        } else {
            Ok(())
        }
    }
}
