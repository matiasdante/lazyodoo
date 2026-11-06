//! Idiomas de la interfaz. El texto en inglés es la clave: `tr` lo busca en `ui_strings`
//! y, si no hay traducción, lo devuelve tal cual (un test exige que no falte ninguna).
//!
//! El idioma activo es global porque los mensajes de error también se arman en los threads
//! que hablan con Odoo. Solo lo cambia `main` (al elegirlo); los tests usan `tr_in`.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Es,
    Pt,
    Zh,
}

impl Lang {
    pub const ALL: [Lang; 4] = [Lang::En, Lang::Es, Lang::Pt, Lang::Zh];

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }

    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::Pt => "Português",
            Lang::Zh => "中文",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Lang {
    Lang::ALL[CURRENT.load(Ordering::Relaxed) as usize % Lang::ALL.len()]
}

pub fn set(lang: Lang) {
    CURRENT.store(lang.index() as u8, Ordering::Relaxed);
}

/// Traduce al idioma activo.
pub fn tr(en: &'static str) -> &'static str {
    tr_in(current(), en)
}

pub fn tr_in(lang: Lang, en: &'static str) -> &'static str {
    crate::ui_strings::lookup(lang, en).unwrap_or(en)
}

/// Traduce y completa los `{nombre}` del texto.
pub fn trf(en: &'static str, args: &[(&str, &str)]) -> String {
    fill(tr(en), args)
}

pub fn fill(text: &str, args: &[(&str, &str)]) -> String {
    args.iter().fold(text.to_string(), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_cycle_in_both_directions() {
        assert_eq!(Lang::En.next(), Lang::Es);
        assert_eq!(Lang::Zh.next(), Lang::En);
        assert_eq!(Lang::En.prev(), Lang::Zh);
        for lang in Lang::ALL {
            assert_eq!(lang.next().prev(), lang);
        }
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(
            fill("{a} y {b} y {a}", &[("a", "1"), ("b", "2")]),
            "1 y 2 y 1"
        );
    }
}
