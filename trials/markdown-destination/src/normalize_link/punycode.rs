//! Reachable toASCII path from punycode.js 2.3.1; see ../../NOTICES.md.

const MAX_INT: u64 = 2_147_483_647;

pub(super) fn to_ascii(input: &str) -> Option<String> {
    // mapDomain keeps only the first domain after an @, as in pinned JS.
    let mut parts = input.split('@');
    let first = parts.next().unwrap_or_default();
    let (prefix, domain) = match parts.next() {
        Some(domain) => (format!("{first}@"), domain),
        None => (String::new(), first),
    };
    let domain = domain.replace(['\u{3002}', '\u{ff0e}', '\u{ff61}'], ".");
    let mut output = prefix;
    for (i, label) in domain.split('.').enumerate() {
        if i > 0 {
            output.push('.');
        }
        if label.is_ascii() {
            output.push_str(label);
        } else {
            output.push_str("xn--");
            output.push_str(&encode(label)?);
        }
    }
    Some(output)
}

fn adapt(mut delta: u64, points: u64, first: bool) -> u64 {
    delta = if first { delta / 700 } else { delta >> 1 };
    delta += delta / points;
    let mut k = 0;
    while delta > 455 {
        delta /= 35;
        k += 36;
    }
    k + 36 * delta / (delta + 38)
}

fn digit(value: u64) -> char {
    // Each caller supplies the Bootstring digit 0..35.
    (if value < 26 {
        b'a' + value as u8
    } else {
        b'0' + (value - 26) as u8
    }) as char
}

fn encode(input: &str) -> Option<String> {
    // Rust chars already give ucs2decode's code points for valid Rust strings.
    let input: Vec<u64> = input.chars().map(|c| u64::from(u32::from(c))).collect();
    let mut output: String = input
        .iter()
        .filter(|&&c| c < 128)
        .map(|&c| c as u8 as char)
        .collect();
    let basic = output.len() as u64;
    let mut handled = basic;
    if basic > 0 {
        output.push('-');
    }
    let mut n = 128;
    let mut delta = 0;
    let mut bias = 72;
    while handled < input.len() as u64 {
        if delta > MAX_INT {
            return None;
        }
        let m = input.iter().copied().filter(|&c| c >= n).min()?;
        let points = handled + 1;
        if m - n > (MAX_INT - delta) / points {
            return None;
        }
        delta += (m - n) * points;
        n = m;
        for &c in &input {
            if c < n {
                delta += 1;
                if delta > MAX_INT {
                    return None;
                }
            }
            if c == n {
                let mut q = delta;
                let mut k = 36;
                loop {
                    let t = if k <= bias {
                        1
                    } else if k >= bias + 26 {
                        26
                    } else {
                        k - bias
                    };
                    if q < t {
                        break;
                    }
                    output.push(digit(t + (q - t) % (36 - t)));
                    q = (q - t) / (36 - t);
                    k += 36;
                }
                output.push(digit(q));
                bias = adapt(delta, points, handled == basic);
                delta = 0;
                handled += 1;
            }
        }
        delta += 1;
        n += 1;
    }
    Some(output)
}
