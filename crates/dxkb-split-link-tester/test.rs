fn seq_diff(new: u8, old: u8) -> i8 {
    new.wrapping_sub(old) as i8
}

fn main() {
    println!("{}", seq_diff(200, 255));
    println!("{}", seq_diff(100, 150));
    println!("{}", seq_diff(100, 250));
    println!("{}", seq_diff(250, 100));
}
