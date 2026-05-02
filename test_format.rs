fn main() {
    let css = std::fs::read_to_string("src/ui/styles.css").unwrap();
    let head = format!(r##"<style>{}</style>"##, css);
    println!("Length: {}", head.len());
    println!("Ends with: {}", &head[head.len()-20..]);
}
