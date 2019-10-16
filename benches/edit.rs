#![feature(test)]

extern crate test;

use kiro_editor::{Editor, InputSeq, KeySeq, Result, StdinRawMode};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::io;
use test::Bencher;

fn gen_printable_ascii_char<R: Rng>(rng: &mut R) -> u8 {
    rng.gen_range(0x20, 0x7f)
}

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

        lines
            .last_mut()
            .unwrap()
            .push(gen_printable_ascii_char(&mut rng) as char);
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

    fn gen_random_key(&mut self, normal_keys: &[u8], special_keys: &[KeySeq]) -> KeySeq {
        let len_normal_keys = normal_keys.len();
        let len_special_keys = special_keys.len();
        if self.rng.gen_range(0, len_normal_keys + len_special_keys) < len_normal_keys {
            let b = normal_keys[self.rng.gen_range(0, len_normal_keys)].clone();
            KeySeq::Key(b)
        } else {
            special_keys[self.rng.gen_range(0, len_special_keys)].clone()
        }
    }

    fn random_key(&mut self) -> InputSeq {
        match self.rng.gen_range(0, 100) {
            0..=9 => InputSeq {
                key: self.gen_random_key(VALID_CTRL_KEYS, VALID_CTRL_SPECIAL_KEYS),
                ctrl: true,
                alt: false,
            },
            10..=19 => InputSeq {
                key: self.gen_random_key(VALID_ALT_KEYS, VALID_ALT_SPECIAL_KEYS),
                ctrl: false,
                alt: true,
            },
            20..=29 => InputSeq {
                key: VALID_SPECIAL_KEYS[self.rng.gen_range(0, VALID_SPECIAL_KEYS.len())].clone(),
                ctrl: false,
                alt: false,
            },
            _ => InputSeq {
                key: KeySeq::Key(gen_printable_ascii_char(&mut self.rng)),
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

#[bench]
fn bench_1000_operations_to_10000_chars_text(b: &mut Bencher) {
    let lines = generate_random_text(10000);
    let input = RandomInput::new(1000);
    b.iter(|| {
        let _stdin = StdinRawMode::new().unwrap();
        let mut editor =
            Editor::with_lines(lines.iter(), input.clone(), io::stdout(), Some((80, 24))).unwrap();
        editor.edit().unwrap();
    });
}
