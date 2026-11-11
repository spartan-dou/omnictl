//! JSON minimal (lecture et écriture), suffisant pour le protocole MCP.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    /// Paires dans l'ordre du document.
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Entier dans 0..=255, refusé s'il a une partie décimale ou sort de l'intervalle.
    pub fn as_u8(&self) -> Option<u8> {
        match self {
            Value::Num(n) if n.fract() == 0.0 && (0.0..=255.0).contains(n) => Some(*n as u8),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Obj(o) => Some(o),
            _ => None,
        }
    }

    pub fn write(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Num(n) if n.fract() == 0.0 && n.abs() < 9e15 => out.push_str(&format!("{}", *n as i64)),
            Value::Num(n) if n.is_finite() => out.push_str(&n.to_string()),
            Value::Num(_) => out.push_str("null"),
            Value::Str(s) => write_str(s, out),
            Value::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Value::Obj(pairs) => {
                out.push('{');
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_str(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

pub fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    write_str(s, &mut out);
    out
}

const MAX_DEPTH: usize = 64;

pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser { s: text.as_bytes(), i: 0 };
    let v = p.value(0)?;
    p.ws();
    if p.i != p.s.len() {
        return Err(format!("caractères en trop à la position {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while matches!(self.s.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} à la position {}", self.i))
    }

    fn eat(&mut self, lit: &str) -> bool {
        if self.s[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return self.err("imbrication trop profonde");
        }
        self.ws();
        match self.s.get(self.i) {
            None => self.err("fin inattendue"),
            Some(b'{') => {
                self.i += 1;
                let mut pairs = Vec::new();
                self.ws();
                if self.eat("}") {
                    return Ok(Value::Obj(pairs));
                }
                loop {
                    self.ws();
                    let key = self.string()?;
                    self.ws();
                    if !self.eat(":") {
                        return self.err("« : » attendu");
                    }
                    pairs.push((key, self.value(depth + 1)?));
                    self.ws();
                    if self.eat("}") {
                        return Ok(Value::Obj(pairs));
                    }
                    if !self.eat(",") {
                        return self.err("« , » ou « } » attendu");
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.eat("]") {
                    return Ok(Value::Arr(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    if self.eat("]") {
                        return Ok(Value::Arr(items));
                    }
                    if !self.eat(",") {
                        return self.err("« , » ou « ] » attendu");
                    }
                }
            }
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') if self.eat("true") => Ok(Value::Bool(true)),
            Some(b'f') if self.eat("false") => Ok(Value::Bool(false)),
            Some(b'n') if self.eat("null") => Ok(Value::Null),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.i;
                while matches!(self.s.get(self.i), Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
                    self.i += 1;
                }
                // Tranche ASCII par construction.
                let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
                match text.parse::<f64>() {
                    Ok(n) => Ok(Value::Num(n)),
                    Err(_) => self.err("nombre invalide"),
                }
            }
            Some(_) => self.err("valeur inattendue"),
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.s.get(self.i..self.i + 4).and_then(|h| std::str::from_utf8(h).ok());
        match digits.and_then(|h| u32::from_str_radix(h, 16).ok()) {
            Some(v) => {
                self.i += 4;
                Ok(v)
            }
            None => self.err("échappement \\u invalide"),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        if !self.eat("\"") {
            return self.err("chaîne attendue");
        }
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&b) = self.s.get(self.i) else {
                return self.err("chaîne non terminée");
            };
            self.i += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.s.get(self.i) else {
                        return self.err("chaîne non terminée");
                    };
                    self.i += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let mut cp = self.hex4()?;
                            // Paire de substitution UTF-16.
                            if (0xD800..0xDC00).contains(&cp) && self.eat("\\u") {
                                let low = self.hex4()?;
                                if (0xDC00..0xE000).contains(&low) {
                                    cp = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                                }
                            }
                            char::from_u32(cp).unwrap_or('\u{FFFD}')
                        }
                        _ => return self.err("échappement invalide"),
                    };
                    out.extend_from_slice(c.encode_utf8(&mut [0u8; 4]).as_bytes());
                }
                b => out.push(b),
            }
        }
        String::from_utf8(out).or_else(|_| self.err("UTF-8 invalide"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let text = r#"{"a":[1,2.5,-3],"b":"é\n\"x\"","c":{"d":null,"e":true}}"#;
        let v = parse(text).unwrap();
        assert_eq!(v.get("b").and_then(Value::as_str), Some("é\n\"x\""));
        assert_eq!(v.get("a").and_then(Value::as_array).map(<[Value]>::len), Some(3));
        let mut out = String::new();
        v.write(&mut out);
        assert_eq!(parse(&out).unwrap(), v);
    }

    #[test]
    fn unicode_escapes() {
        assert_eq!(parse(r#""é😀""#).unwrap(), Value::Str("é😀".into()));
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["", "{", "[1,]", "{\"a\"}", "tru", "1 2", "\"abc"] {
            assert!(parse(bad).is_err(), "{bad:?} aurait dû être refusé");
        }
    }

    #[test]
    fn u8_bounds() {
        assert_eq!(Value::Num(255.0).as_u8(), Some(255));
        assert_eq!(Value::Num(256.0).as_u8(), None);
        assert_eq!(Value::Num(1.5).as_u8(), None);
        assert_eq!(Value::Num(-1.0).as_u8(), None);
    }
}
