fn main() {
    let api = hidapi::HidApi::new().unwrap();
    println!("Found {} HID devices total", api.device_list().count());
    println!();
    for d in api.device_list() {
        if d.vendor_id() == 0x046D {
            println!(
                "Logitech: VID={:04x} PID={:04x} usage_page=0x{:04x} usage=0x{:04x} product={:?} path={:?}",
                d.vendor_id(),
                d.product_id(),
                d.usage_page(),
                d.usage(),
                d.product_string().unwrap_or("?"),
                d.path()
            );
        }
    }
}
