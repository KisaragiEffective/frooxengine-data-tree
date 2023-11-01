use std::io::BufReader;

fn main() {
    let file = std::fs::read("/home/you/.config/Example/example.brson").expect("fail");
    let m = frooxengine_data_tree::split_froox_container_header(&file).unwrap_or_else(|_| frooxengine_data_tree::legacy(&file));
    let b = m.deserialize::<bson::Bson>().expect("fail");
    println!("{b:?}");
}
