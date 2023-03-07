#[allow(dead_code)]
struct Bluetooth16bitUUID {
    value: u16,
}

#[allow(dead_code)]
pub enum Bluetooth16bitUUIDEnum {
    Headset = 0x1108,                   // Service Class and Profile
    AudioSink = 0x110b,                 //  AudioSink Service Class
    AdvancedAudioDistribution = 0x110d, // Profile
    AVRemoteControl = 0x110e,           // Service Class and Profile
    AVRemoteControlTarget = 0x110c,     //  Service Class
    Handsfree = 0x111e,
    AVRemoteControlController = 0x110f, // Service Class
    HeadsetHS = 0x1131,                 //­ HS Service Class
}
