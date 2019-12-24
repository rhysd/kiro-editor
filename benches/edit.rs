#![feature(test)]

extern crate test;

use kiro_editor::{Editor, InputSeq, KeySeq, Language, Result, StdinRawMode};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::fs::File;
use std::io::{self, Write};
use std::io::{BufRead, BufReader};
use std::path::Path;
use test::Bencher;

fn generate_random_text(max_chars: usize) -> Vec<String> {
    let max_chars_in_line = 200;
    let mut lines = vec!["".to_string()];
    let seed = [0; 32];
    let mut rng: StdRng = SeedableRng::from_seed(seed);
    let mut rest = rng.gen_range(0, max_chars_in_line);
    let mut chars = 0;
    loop {
        if chars > max_chars {
            return lines;
        }
        chars += 1;

        if rest == 0 {
            lines.push("".to_string());
            rest = rng.gen_range(0, max_chars_in_line);
            continue;
        }

        let b: u8 = rng.gen_range(0x20, 0x7f);
        lines.last_mut().unwrap().push(b as char); // Generate ascii printable char
        rest -= 1;
    }
}

// Ctrl-G and Ctrl-S are not included because they quits an editor and saves a file
const VALID_CTRL_KEYS: &[u8] = b"pbnfvaedghkjwlimo?x]ur";
const VALID_ALT_KEYS: &[u8] = b"vfbnpx<>";
const VALID_SPECIAL_KEYS: &[KeySeq] = &[
    KeySeq::LeftKey,
    KeySeq::RightKey,
    KeySeq::DownKey,
    KeySeq::UpKey,
    KeySeq::PageUpKey,
    KeySeq::PageDownKey,
    KeySeq::HomeKey,
    KeySeq::EndKey,
    KeySeq::DeleteKey,
];
const VALID_CTRL_SPECIAL_KEYS: &[KeySeq] = &[
    KeySeq::LeftKey,
    KeySeq::RightKey,
    KeySeq::DownKey,
    KeySeq::UpKey,
];
const VALID_ALT_SPECIAL_KEYS: &[KeySeq] = &[KeySeq::LeftKey, KeySeq::RightKey];

#[derive(Clone)]
struct RandomInput {
    rng: StdRng,
    rest_steps: usize,
}

impl RandomInput {
    fn new(num_steps: usize) -> Self {
        let seed = [0; 32];
        RandomInput {
            rng: SeedableRng::from_seed(seed),
            rest_steps: num_steps,
        }
    }

    fn gen_random_key_from(&mut self, normal_keys: &[u8], special_keys: &[KeySeq]) -> KeySeq {
        let len_normal_keys = normal_keys.len();
        let len_special_keys = special_keys.len();
        if self.rng.gen_range(0, len_normal_keys + len_special_keys) < len_normal_keys {
            let b = normal_keys[self.rng.gen_range(0, len_normal_keys)].clone();
            KeySeq::Key(b)
        } else {
            special_keys[self.rng.gen_range(0, len_special_keys)].clone()
        }
    }

    fn gen_normal_ascii_input(&mut self) -> KeySeq {
        let b = match self.rng.gen_range(0x1f, 0x80) {
            0x1f => b'\r',
            b => b,
        };
        KeySeq::Key(b)
    }

    fn random_key(&mut self) -> InputSeq {
        match self.rng.gen_range(0, 100) {
            0..=4 => InputSeq {
                key: self.gen_random_key_from(VALID_CTRL_KEYS, VALID_CTRL_SPECIAL_KEYS),
                ctrl: true,
                alt: false,
            },
            5..=9 => InputSeq {
                key: self.gen_random_key_from(VALID_ALT_KEYS, VALID_ALT_SPECIAL_KEYS),
                ctrl: false,
                alt: true,
            },
            10..=14 => InputSeq {
                key: VALID_SPECIAL_KEYS[self.rng.gen_range(0, VALID_SPECIAL_KEYS.len())].clone(),
                ctrl: false,
                alt: false,
            },
            _ => InputSeq {
                key: self.gen_normal_ascii_input(),
                ctrl: false,
                alt: false,
            },
        }
    }
}

impl Iterator for RandomInput {
    type Item = Result<InputSeq>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest_steps == 0 {
            return None;
        }
        self.rest_steps -= 1;

        Some(Ok(self.random_key()))
    }
}

// TODO: Move to helper
pub struct Discard;

impl Write for Discard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[bench]
fn with_term_edit_1000_operations_to_10000_chars_plain_text(b: &mut Bencher) -> Result<()> {
    let lines = generate_random_text(10000);
    let input = RandomInput::new(1000);
    let _stdin = StdinRawMode::new()?;
    b.iter(|| {
        let mut editor =
            Editor::with_lines(lines.iter(), input.clone(), io::stdout(), Some((80, 24))).unwrap();
        editor.edit().unwrap();
    });
    Ok(())
}

#[bench]
fn no_term_edit_1000_operations_to_10000_chars_plain_text(b: &mut Bencher) {
    let lines = generate_random_text(10000);
    let input = RandomInput::new(1000);
    b.iter(|| {
        let mut editor =
            Editor::with_lines(lines.iter(), input.clone(), Discard, Some((80, 24))).unwrap();
        editor.edit().unwrap();
    });
}

#[bench]
fn with_term_edit_1000_operations_to_editor_rs(b: &mut Bencher) -> Result<()> {
    let f = BufReader::new(File::open(&Path::new("src/editor.rs"))?);
    let lines = f.lines().collect::<io::Result<Vec<_>>>()?;
    let input = RandomInput::new(1000);
    let _stdin = StdinRawMode::new()?;
    b.iter(|| {
        let mut editor =
            Editor::with_lines(lines.iter(), input.clone(), io::stdout(), Some((80, 24))).unwrap();
        editor.set_lang(Language::Rust);
        editor.edit().unwrap();
    });
    Ok(())
}

#[bench]
fn no_term_edit_1000_operations_to_editor_rs(b: &mut Bencher) -> Result<()> {
    let f = BufReader::new(File::open(&Path::new("src/editor.rs"))?);
    let lines = f.lines().collect::<io::Result<Vec<_>>>()?;
    let input = RandomInput::new(1000);
    b.iter(|| {
        let mut editor =
            Editor::with_lines(lines.iter(), input.clone(), Discard, Some((80, 24))).unwrap();
        editor.set_lang(Language::Rust);
        editor.edit().unwrap();
    });
    Ok(())
}
