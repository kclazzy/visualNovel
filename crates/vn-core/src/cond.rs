//! Условия над переменными: `доверие >= 5 и не ключ или деньги < 10`.
//! В данных хранится дерево, текст разбирается один раз — в редакторе.
use crate::model::{Value, Vars};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Cond {
    Cmp { var: String, cmp: Cmp, value: Value },
    And { items: Vec<Cond> },
    Or { items: Vec<Cond> },
    Not { cond: Box<Cond> },
}

impl Cond {
    pub fn eval(&self, vars: &Vars) -> bool {
        match self {
            Cond::Cmp { var, cmp, value } => vars.get(var).is_some_and(|v| compare(v, *cmp, value)),
            Cond::And { items } => items.iter().all(|c| c.eval(vars)),
            Cond::Or { items } => items.iter().any(|c| c.eval(vars)),
            Cond::Not { cond } => !cond.eval(vars),
        }
    }

    pub fn parse(text: &str) -> Result<Cond, String> {
        let tokens = tokenize(text)?;
        let mut p = Parser { tokens, at: 0 };
        let c = p.or()?;
        match p.peek() {
            None => Ok(c),
            Some(t) => Err(format!("лишнее в условии: {t:?}")),
        }
    }

    /// Имена всех переменных, которые читает условие.
    pub fn vars(&self) -> Vec<&str> {
        match self {
            Cond::Cmp { var, .. } => vec![var.as_str()],
            Cond::And { items } | Cond::Or { items } => items.iter().flat_map(|c| c.vars()).collect(),
            Cond::Not { cond } => cond.vars(),
        }
    }
}

fn compare(a: &Value, cmp: Cmp, b: &Value) -> bool {
    use std::cmp::Ordering::*;
    let ord = match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Text(x), Value::Text(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) if matches!(cmp, Cmp::Eq | Cmp::Ne) => x.cmp(y),
        _ => return cmp == Cmp::Ne,
    };
    match cmp {
        Cmp::Eq => ord == Equal,
        Cmp::Ne => ord != Equal,
        Cmp::Gt => ord == Greater,
        Cmp::Lt => ord == Less,
        Cmp::Ge => ord != Less,
        Cmp::Le => ord != Greater,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Open,
    Close,
    Cmp(Cmp),
    And,
    Or,
    Not,
    Word(String),
    Quoted(String),
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

fn tokenize(text: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let (tok, len) = match (c, next) {
            (c, _) if c.is_whitespace() => {
                i += 1;
                continue;
            }
            ('(', _) => (Tok::Open, 1),
            (')', _) => (Tok::Close, 1),
            ('=', Some('=')) => (Tok::Cmp(Cmp::Eq), 2),
            ('=', _) => (Tok::Cmp(Cmp::Eq), 1),
            ('!', Some('=')) => (Tok::Cmp(Cmp::Ne), 2),
            ('>', Some('=')) => (Tok::Cmp(Cmp::Ge), 2),
            ('<', Some('=')) => (Tok::Cmp(Cmp::Le), 2),
            ('>', _) => (Tok::Cmp(Cmp::Gt), 1),
            ('<', _) => (Tok::Cmp(Cmp::Lt), 1),
            ('&', Some('&')) => (Tok::And, 2),
            ('|', Some('|')) => (Tok::Or, 2),
            ('!', _) => (Tok::Not, 1),
            ('"', _) => {
                let end = chars[i + 1..].iter().position(|&c| c == '"').ok_or("нет закрывающей кавычки")?;
                (Tok::Quoted(chars[i + 1..i + 1 + end].iter().collect()), end + 2)
            }
            (c, n) if is_word_char(c) || (c == '-' && n.is_some_and(|n| n.is_ascii_digit())) => {
                let len = 1 + chars[i + 1..].iter().take_while(|&&c| is_word_char(c)).count();
                let word: String = chars[i..i + len].iter().collect();
                let tok = match word.to_lowercase().as_str() {
                    "and" | "и" => Tok::And,
                    "or" | "или" => Tok::Or,
                    "not" | "не" => Tok::Not,
                    _ => Tok::Word(word),
                };
                (tok, len)
            }
            (c, _) => return Err(format!("непонятный символ «{c}»")),
        };
        out.push(tok);
        i += len;
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at)
    }
    fn next(&mut self) -> Option<Tok> {
        self.at += 1;
        self.tokens.get(self.at - 1).cloned()
    }
    fn list(&mut self, sep: Tok, item: fn(&mut Self) -> Result<Cond, String>) -> Result<Vec<Cond>, String> {
        let mut items = vec![item(self)?];
        while self.peek() == Some(&sep) {
            self.at += 1;
            items.push(item(self)?);
        }
        Ok(items)
    }
    fn or(&mut self) -> Result<Cond, String> {
        let mut items = self.list(Tok::Or, Self::and)?;
        Ok(if items.len() == 1 { items.remove(0) } else { Cond::Or { items } })
    }
    fn and(&mut self) -> Result<Cond, String> {
        let mut items = self.list(Tok::And, Self::not)?;
        Ok(if items.len() == 1 { items.remove(0) } else { Cond::And { items } })
    }
    fn not(&mut self) -> Result<Cond, String> {
        match self.next() {
            Some(Tok::Not) => Ok(Cond::Not { cond: Box::new(self.not()?) }),
            Some(Tok::Open) => {
                let c = self.or()?;
                match self.next() {
                    Some(Tok::Close) => Ok(c),
                    _ => Err("не хватает закрывающей скобки".into()),
                }
            }
            Some(Tok::Word(var)) => {
                let Some(Tok::Cmp(cmp)) = self.peek().cloned() else {
                    return Ok(Cond::Cmp { var, cmp: Cmp::Eq, value: Value::Bool(true) });
                };
                self.at += 1;
                let value = match self.next() {
                    Some(Tok::Word(w)) => Value::parse(&w),
                    Some(Tok::Quoted(q)) => Value::Text(q),
                    _ => return Err(format!("после сравнения «{var}» нужно значение")),
                };
                Ok(Cond::Cmp { var, cmp, value })
            }
            Some(t) => Err(format!("ожидалось имя переменной, а не {t:?}")),
            None => Err("условие пустое или оборвано".into()),
        }
    }
}

impl std::fmt::Display for Cond {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let join = |f: &mut std::fmt::Formatter<'_>, items: &[Cond], sep: &str, wrap: fn(&Cond) -> bool| {
            for (i, c) in items.iter().enumerate() {
                if i > 0 {
                    f.write_str(sep)?;
                }
                if wrap(c) {
                    write!(f, "({c})")?
                } else {
                    write!(f, "{c}")?
                }
            }
            Ok(())
        };
        match self {
            Cond::Cmp { var, cmp, value } => {
                let op = match cmp {
                    Cmp::Eq => "==",
                    Cmp::Ne => "!=",
                    Cmp::Gt => ">",
                    Cmp::Lt => "<",
                    Cmp::Ge => ">=",
                    Cmp::Le => "<=",
                };
                match value {
                    Value::Text(t) if Value::parse(t) != *value || !t.chars().all(is_word_char) || t.is_empty() || tokenize(t).ok() != Some(vec![Tok::Word(t.clone())]) => write!(f, "{var} {op} \"{t}\""),
                    v => write!(f, "{var} {op} {v}"),
                }
            }
            Cond::And { items } => join(f, items, " и ", |c| matches!(c, Cond::Or { .. })),
            Cond::Or { items } => join(f, items, " или ", |_| false),
            Cond::Not { cond } => match **cond {
                Cond::Cmp { .. } | Cond::Not { .. } => write!(f, "не {cond}"),
                _ => write!(f, "не ({cond})"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars {
        [("trust".to_string(), Value::Int(5)), ("key".to_string(), Value::Bool(true)), ("outfit".to_string(), Value::Text("outfit_03".into())), ("доверие".to_string(), Value::Int(2))].into_iter().collect()
    }

    fn check(text: &str) -> bool {
        Cond::parse(text).unwrap().eval(&vars())
    }

    #[test]
    fn comparisons() {
        assert!(check("trust == 5"));
        assert!(check("trust >= 5"));
        assert!(!check("trust > 5"));
        assert!(check("trust != 4"));
        assert!(check("trust <= 5 "));
        assert!(!check("trust < 5"));
        assert!(check("outfit == outfit_03"));
        assert!(check("outfit == \"outfit_03\""));
    }

    #[test]
    fn bare_name_means_true() {
        assert!(check("key"));
        assert!(!check("не key"));
        assert!(!check("missing"));
    }

    #[test]
    fn logic_with_precedence_and_russian_words() {
        assert!(check("trust > 9 или key и доверие == 2"));
        assert!(!check("(trust > 9 or key) and доверие == 3"));
        assert!(check("!(trust > 9) && key"));
    }

    #[test]
    fn mismatched_types_are_not_equal() {
        assert!(!check("key == 1"));
        assert!(check("key != 1"));
        assert!(!check("outfit > 3"));
    }

    #[test]
    fn errors_are_reported() {
        assert!(Cond::parse("trust >=").is_err());
        assert!(Cond::parse("(trust > 1").is_err());
        assert!(Cond::parse("").is_err());
        assert!(Cond::parse("a b").is_err());
    }

    #[test]
    fn display_round_trips() {
        for text in ["trust >= 5", "не key", "a == 1 и (b или не c)", "name == \"два слова\"", "x == -3"] {
            let c = Cond::parse(text).unwrap();
            assert_eq!(Cond::parse(&c.to_string()).unwrap(), c, "{text} → {c}");
        }
    }

    #[test]
    fn lists_variables() {
        assert_eq!(Cond::parse("a > 1 и (b или не c)").unwrap().vars(), vec!["a", "b", "c"]);
    }
}
