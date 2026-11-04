use regex::Regex;
use std::{env, fs::write, path::Path};

// This is so jank...
// Keycodes from: https://github.com/torvalds/linux/blob/master/include/uapi/linux/input-event-codes.h

const KEYCODE_DEFINITIONS: &str = "
#define KEY_Q			16
#define KEY_W			17
#define KEY_E			18
#define KEY_R			19
#define KEY_T			20
#define KEY_Y			21
#define KEY_U			22
#define KEY_I			23
#define KEY_O			24
#define KEY_P			25

#define KEY_A			30
#define KEY_S			31
#define KEY_D			32
#define KEY_F			33
#define KEY_G			34
#define KEY_H			35
#define KEY_J			36
#define KEY_K			37
#define KEY_L			38

#define KEY_Z			44
#define KEY_X			45
#define KEY_C			46
#define KEY_V			47
#define KEY_B			48
#define KEY_N			49
#define KEY_M			50
";

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("keycode_matches.rs");

    // Wrote this Regex myself, based on https://docs.rs/regex/latest/regex/struct.Regex.html
    // pretty proud of myself
    let re = Regex::new(r"(?m)^\s*#define\s+KEY_(\S)\s+([0-9]+)\s*$").unwrap();

    let match_arms = re
        .captures_iter(KEYCODE_DEFINITIONS)
        .filter_map(|captures| {
            let key_char = captures[1].chars().next()?.to_lowercase();
            let keycode: u32 = captures[2].parse().ok()?;

            Some(format!("KeySymbol::Char('{key_char}') => {keycode}"))
        })
        .collect::<Vec<_>>()
        .join(",\n");

    let content = format!(
        r"
            match key {{
                KeySymbol::ArrowKeyUp => 103,
                KeySymbol::ArrowKeyDown => 108,
                KeySymbol::ArrowKeyLeft => 105,
                KeySymbol::ArrowKeyRight => 106,
                KeySymbol::Enter => 28,
                KeySymbol::Backspace => 14,
                KeySymbol::Char(' ') => 57,
                KeySymbol::Char('0') => 11,
                // KEY_1 = 2 and etc
                KeySymbol::Char(c @ '1'..='9') => c.to_digit(10)? + 1,
                {match_arms},
                _ => None?,
            }}
        "
    );
    write(dest_path, content).unwrap();

    // Re-run if build.rs itself changes
    println!("cargo:rerun-if-changed=build.rs");
}
