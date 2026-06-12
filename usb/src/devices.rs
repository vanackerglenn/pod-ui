use once_cell::sync::Lazy;
use rusb::TransferType;

pub enum UsbId {
    Device { vid: u16, pid: u16 },
    DeviceInterface { vid: u16, pid: u16, iface: u8 }
}

pub trait UsbIdOps {
    fn vid(&self) -> u16;
    fn pid(&self) -> u16;
}

impl UsbIdOps for UsbId {
    fn vid(&self) -> u16 {
        match self {
            UsbId::Device { vid, .. } => *vid,
            UsbId::DeviceInterface { vid, .. } => *vid
        }
    }

    fn pid(&self) -> u16 {
        match self {
            UsbId::Device { pid, .. } => *pid,
            UsbId::DeviceInterface { pid, .. } => *pid
        }
    }
}

macro_rules! id {
    ($vid:tt, $pid:tt) => (
        UsbId::Device { vid: $vid, pid: $pid }
    );
    ($vid:tt, $pid:tt, $iface:tt) => (
        UsbId::DeviceInterface { vid: $vid, pid: $pid, iface: $iface }
    );
}

pub struct UsbDevice {
    pub id: UsbId,
    pub name: String,
    pub alt_setting: u8,
    pub read_ep: u8,
    pub write_ep: u8,
    pub transfer_type: TransferType,
    pub config_name: Option<&'static str>,
}

// based on: https://github.com/torvalds/linux/blob/8508fa2e7472f673edbeedf1b1d2b7a6bb898ecc/sound/usb/line6/pod.c
static USB_DEVICES: Lazy<Vec<UsbDevice>> = Lazy::new(|| {
    vec![
        UsbDevice {
            id: id!(0x0e41, 0x5044), name: "POD XT".into(),
            alt_setting: 5, read_ep: 0x84, write_ep: 0x03,
            transfer_type: TransferType::Interrupt,
            config_name: Some("PODxt")
        },
        UsbDevice {
            id: id!(0x0e41, 0x5050), name: "POD XT Pro".into(),
            alt_setting: 5, read_ep: 0x84, write_ep: 0x03,
            transfer_type: TransferType::Interrupt,
            config_name: Some("PODxt Pro")
        },
        UsbDevice {
            id: id!(0x0e41, 0x4650, 0), name: "POD XT Live".into(),
            alt_setting: 1, read_ep: 0x84, write_ep: 0x03,
            transfer_type: TransferType::Interrupt,
            config_name: Some("PODxt Live")
        },
        UsbDevice {
            id: id!(0x0e41, 0x4250), name: "Bass POD XT".into(),
            alt_setting: 5, read_ep: 0x84, write_ep: 0x03,
            transfer_type: TransferType::Interrupt,
            config_name: Some("Bass POD XT")
        },
        UsbDevice {
            id: id!(0x0e41, 0x4252), name: "Bass POD XT Pro".into(),
            alt_setting: 5, read_ep: 0x84, write_ep: 0x03,
            transfer_type: TransferType::Interrupt,
            config_name: Some("Bass POD XT Pro")
        },
        UsbDevice {
            id: id!(0x0e41, 0x4642), name: "Bass POD XT Live".into(),
            alt_setting: 1, read_ep: 0x84, write_ep: 0x03,
            transfer_type: TransferType::Interrupt,
            config_name: Some("Bass POD XT Live")
        },
        UsbDevice {
            id: id!(0x0e41, 0x4247), name: "POD Go".into(),
            alt_setting: 0, read_ep: 0x82, write_ep: 0x02,
            transfer_type: TransferType::Bulk,
            config_name: Some("POD Go")
        },
        UsbDevice {
            id: id!(0x0e41, 0x5051, 1), name: "Pocket POD".into(),
            alt_setting: 0, read_ep: 0x82, write_ep: 0x02,
            transfer_type: TransferType::Bulk,
            config_name: Some("Pocket POD")
        },
        UsbDevice {
            id: id!(0x0010, 0x0001), name: "POD-UI testing device".into(),
            alt_setting: 0, read_ep: 0x81, write_ep: 0x02,
            transfer_type: TransferType::Bulk,
            config_name: None
        },
    ]
});

pub fn find_device(vid: u16, pid: u16) -> Option<&'static UsbDevice> {
  USB_DEVICES.iter().find(|d| d.id.vid() == vid && d.id.pid() == pid)
}

pub fn find_devices(vid: u16, pid: u16) -> Vec<&'static UsbDevice> {
  USB_DEVICES.iter().filter(|d| d.id.vid() == vid && d.id.pid() == pid).collect()
}

pub fn find_device_by_name(name: &str) -> Option<&'static UsbDevice> {
  USB_DEVICES.iter().find(|d| {
      let base = d.name.as_str();
      name == base || (name.starts_with(base) && name.as_bytes().get(base.len()) == Some(&b' '))
  })
}