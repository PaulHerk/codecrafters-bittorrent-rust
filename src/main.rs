use serde_json::{self, Map};
use std::{any::Any, env};

// Available if you need it!

#[allow(dead_code)]
fn decode_bencoded_value(encoded_value: &str) -> (serde_json::Value, &str) {
    match encoded_value.split_at(1) {
        ("i", rest) => {
            // Example: "i5e" -> 5
            if let Some(number) = rest
                .split_once('e')
                .and_then(|(digits, _)| digits.parse::<i64>().ok())
            {
                let num_len_in_ascii = number.to_string().len() + 1; // +1 for the 'e'
                (number.into(), &rest[num_len_in_ascii..])
            } else {
                panic!()
            }
        }
        ("d", mut rest) => {
            // Example: d7:meaningi42e4:wiki7:bencodee -> ["meaning": 42, "wiki": "bencode"]
            let mut dict = Map::new();
            while !rest.is_empty() && !rest.starts_with('e') {
                let (key, remainder) = decode_bencoded_value(rest);
                let Some(key) = key.as_str() else {
                    panic!("Dict keys must be string, not {key:?}")
                };
                let (value, remainder) = decode_bencoded_value(remainder);
                rest = remainder;
                dict.insert(key.to_string(), value);
            }
            (serde_json::Value::Object(dict), &rest[1..])
        }
        ("l", mut rest) => {
            // Example: l7:bencodei-20ee -> ["bencode", -20]
            let mut values = Vec::new();
            while !rest.is_empty() && !rest.starts_with('e') {
                let (value, remainder) = decode_bencoded_value(rest);
                values.push(value);
                rest = remainder;
            }
            (values.into(), &rest[1..])
        }
        _ => {
            // Example: "0:hello" -> "hello"
            if let Some((len, rest)) = encoded_value.split_once(':').and_then(|(len, rest)| {
                let len = len.parse::<usize>().ok()?;
                Some((len, rest))
            }) {
                (rest[..len].to_string().into(), &rest[len..])
            } else {
                panic!("Invalid bencode")
            }
        }
    }
}

// Usage: your_program.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        // You can use print statements as follows for debugging, they'll be visible when running tests.
        eprintln!("Logs from your program will appear here!");

        // Uncomment this block to pass the first stage
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value.0);
    } else {
        println!("unknown command: {}", args[1])
    }
}
