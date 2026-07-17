use std::collections::HashSet;

fn main() {
	let mut types: HashSet<String> = HashSet::new();
	for file in std::fs::read_dir("./benches/recipes").unwrap() {
		let file = file.unwrap();
		let content = std::fs::read_to_string(file.path()).unwrap();
		let value: serde_json::Value = serde_json::from_str(content.as_str()).unwrap();
		let t = value.get("type").unwrap().as_str().unwrap();
		types.insert(t.to_string());
	}

	println!("types:");
	for t in types {
		println!("{t}");
	}

}
