use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;

fn main() -> anyhow::Result<()> {
    let ctx = Context::new()?;
    let devices = ctx.devices()?;

    for dev in devices.iter() {
        let desc = dev.device_descriptor()?;
        if desc.vendor_id() == VID && desc.product_id() == PID {
            println!("Pod Go found:");
            println!("  Bus: {}, Address: {}", dev.bus_number(), dev.address());
            println!("  USB version: {}", desc.usb_version());
            println!();
            for n in 0..desc.num_configurations() {
                let config = dev.config_descriptor(n)?;
                println!("Configuration {}:", config.number());
                for iface in config.interfaces() {
                    for iface_desc in iface.descriptors() {
                        println!("  Interface {}: class={} sub={} proto={}",
                            iface_desc.interface_number(),
                            iface_desc.class_code(),
                            iface_desc.sub_class_code(),
                            iface_desc.protocol_code());
                        for ep in iface_desc.endpoint_descriptors() {
                            let dir = if ep.direction() == rusb::Direction::In { "IN" } else { "OUT" };
                            let transfer = match ep.transfer_type() {
                                rusb::TransferType::Bulk => "Bulk",
                                rusb::TransferType::Interrupt => "Interrupt",
                                rusb::TransferType::Isochronous => "Isochronous",
                                _ => "Other",
                            };
                            println!("    EP 0x{:02x}: {} {} (max_packet={})",
                                ep.address(), dir, transfer, ep.max_packet_size());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
