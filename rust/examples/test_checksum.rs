use mantra::parser::{checksum::calculate_checksum, target::Target};

fn main() {
    let target1 = Target {
        name: "Add".to_string(),
        signature: "func Add(a, b int) int".to_string(),
        instruction: "Add two numbers".to_string(),
    };
    
    let target2 = Target {
        name: "Multiply".to_string(),
        signature: "func Multiply(x, y int) int".to_string(),
        instruction: "Multiply two numbers".to_string(),
    };
    
    let checksum1 = calculate_checksum(&target1);
    let checksum2 = calculate_checksum(&target2);
    
    println!("Add checksum: 0x{:x}", checksum1);
    println!("Multiply checksum: 0x{:x}", checksum2);
}