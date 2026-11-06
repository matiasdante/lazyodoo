//! Campos de texto de los formularios: edición con cursor, desplazamiento
//! horizontal, ajuste de línea en cajas multilínea y la máscara de fecha
//! `DD-MM-AA`. Todas las funciones trabajan sobre un `String` y un cursor en
//! caracteres, para que cada formulario guarde sus valores como siempre.

use chrono::{Datelike, NaiveDate, NaiveTime};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::theme;

// --- edición ----------------------------------------------------------------------

fn byte_at(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// Aplica una tecla de edición (cursor, borrar, escribir). `false` si no la usa.
pub fn edit(text: &mut String, cursor: &mut usize, key: &KeyEvent) -> bool {
    let len = text.chars().count();
    *cursor = (*cursor).min(len);
    match key.code {
        KeyCode::Left => *cursor = cursor.saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor + 1).min(len),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = len,
        KeyCode::Backspace if *cursor > 0 => {
            text.remove(byte_at(text, *cursor - 1));
            *cursor -= 1;
        }
        KeyCode::Delete if *cursor < len => {
            text.remove(byte_at(text, *cursor));
        }
        KeyCode::Backspace | KeyCode::Delete => {}
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) && !c.is_control() => {
            text.insert(byte_at(text, *cursor), c);
            *cursor += 1;
        }
        _ => return false,
    }
    true
}

/// Pegado: sin caracteres de control (no pasa por atajos).
pub fn insert_str(text: &mut String, cursor: &mut usize, pasted: &str) {
    let clean: String = pasted.chars().filter(|c| !c.is_control()).collect();
    let at = (*cursor).min(text.chars().count());
    text.insert_str(byte_at(text, at), &clean);
    *cursor = at + clean.chars().count();
}

// --- vistas -----------------------------------------------------------------------

/// Una línea con desplazamiento horizontal: el texto visible y la columna del
/// cursor, siempre dentro del ancho.
pub fn line_view(text: &str, cursor: usize, width: usize) -> (String, usize) {
    if width == 0 {
        return (String::new(), 0);
    }
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor.min(chars.len());
    let start = if cursor >= width {
        cursor + 1 - width
    } else {
        0
    };
    let end = (start + width).min(chars.len());
    (chars[start..end].iter().collect(), cursor - start)
}

/// Ajuste de línea por palabras: rangos `[inicio, fin)` en caracteres.
pub fn wrap_lines(text: &str, width: usize) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    if n == 0 || width == 0 {
        return vec![(0, 0)];
    }
    let mut lines = vec![];
    let mut start = 0;
    while start < n {
        let mut end = (start + width).min(n);
        // si se corta en medio de una palabra, se retrocede hasta el último espacio
        if end < n
            && chars[end] != ' '
            && let Some(space) = (start..end).rev().find(|&i| chars[i] == ' ')
        {
            end = space + 1;
        }
        lines.push((start, end));
        start = end;
    }
    lines
}

/// Fila y columna del cursor dentro de las líneas ajustadas.
pub fn cursor_pos(lines: &[(usize, usize)], cursor: usize) -> (usize, usize) {
    for (row, &(start, end)) in lines.iter().enumerate() {
        if cursor < end || row + 1 == lines.len() {
            return (row, cursor.saturating_sub(start));
        }
    }
    (0, 0)
}

/// Campo de una línea (con desplazamiento). Sin texto, muestra el `placeholder`.
#[allow(clippy::too_many_arguments)]
pub fn draw_line(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    cursor: usize,
    show_cursor: bool,
    masked: bool,
    placeholder: &str,
    style: Style,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if text.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(placeholder, theme::dim())),
            area,
        );
        if show_cursor {
            frame.set_cursor_position((area.x, area.y));
        }
        return;
    }
    let shown = if masked {
        "•".repeat(text.chars().count())
    } else {
        text.to_string()
    };
    let (view, col) = line_view(&shown, cursor, area.width as usize);
    frame.render_widget(Paragraph::new(Span::styled(view, style)), area);
    if show_cursor {
        frame.set_cursor_position((area.x + col as u16, area.y));
    }
}

/// Caja multilínea con ajuste de línea, desplazamiento vertical y contador.
#[allow(clippy::too_many_arguments)]
pub fn draw_text_box(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    text: &str,
    cursor: usize,
    focused: bool,
    show_cursor: bool,
    max: usize,
) {
    let count = text.chars().count();
    let counter_style = if count > max {
        Style::new().fg(Color::Red)
    } else {
        theme::dim()
    };
    let block = Block::default()
        .title(Line::styled(
            format!(" {title} "),
            if focused {
                theme::heading()
            } else {
                theme::dim()
            },
        ))
        .title_bottom(Line::styled(format!(" {count}/{max} "), counter_style))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 2 || inner.height == 0 {
        return;
    }
    let chars: Vec<char> = text.chars().collect();
    // una columna libre para el cursor al final de una línea llena
    let lines = wrap_lines(text, inner.width as usize - 1);
    let (row, col) = cursor_pos(&lines, cursor.min(chars.len()));
    let scroll = (row + 1).saturating_sub(inner.height as usize);
    let visible: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(inner.height as usize)
        .map(|&(start, end)| Line::raw(chars[start..end].iter().collect::<String>()))
        .collect();
    frame.render_widget(Paragraph::new(visible), inner);
    if show_cursor {
        frame.set_cursor_position((inner.x + col as u16, inner.y + (row - scroll) as u16));
    }
}

/// Barra superior de una pantalla completa: marca, ruta y detalle.
pub fn top_bar(path: &str, detail: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(" lazyodoo ", theme::badge()),
        Span::styled(format!(" {path}"), theme::dim()),
        Span::styled(format!(" {detail}"), theme::heading()),
    ])
}

// --- máscara de fecha DD-MM-AA --------------------------------------------------------

fn digits(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

/// `051126` a `05-11-26`; con grupos completos deja el guion puesto (`05-`).
pub fn mask_format(digits: &str) -> String {
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i == 2 || i == 4 {
            out.push('-');
        }
        out.push(c);
    }
    if matches!(digits.len(), 2 | 4) {
        out.push('-');
    }
    out
}

fn days_in_month(month: u32) -> u32 {
    match month {
        2 => 29,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// ¿Es un prefijo posible de una fecha válida (y no posterior a `max`)?
fn valid_prefix(d: &str, max: Option<NaiveDate>) -> bool {
    let n = |range: std::ops::Range<usize>| d.get(range).and_then(|s| s.parse::<u32>().ok());
    let len = d.len();
    if len >= 1 && n(0..1).is_none_or(|tens| tens > 3) {
        return false;
    }
    if len >= 2 && n(0..2).is_none_or(|day| !(1..=31).contains(&day)) {
        return false;
    }
    if len >= 3 && n(2..3).is_none_or(|tens| tens > 1) {
        return false;
    }
    if len >= 4 {
        let (Some(day), Some(month)) = (n(0..2), n(2..4)) else {
            return false;
        };
        if !(1..=12).contains(&month) || day > days_in_month(month) {
            return false;
        }
    }
    if let Some(max) = max {
        let year = (max.year().rem_euclid(100)) as u32;
        if len >= 5 && n(4..5).is_none_or(|tens| tens > year / 10) {
            return false;
        }
        if len == 6 && n(4..6).is_none_or(|yy| yy > year) {
            return false;
        }
    }
    if len == 6 {
        let (Some(day), Some(month), Some(yy)) = (n(0..2), n(2..4), n(4..6)) else {
            return false;
        };
        let Some(date) = NaiveDate::from_ymd_opt(2000 + yy as i32, month, day) else {
            return false;
        };
        if max.is_some_and(|max| date > max) {
            return false;
        }
    }
    true
}

/// Escribe un dígito en la máscara. `None` = la tecla se rechaza (no es un
/// dígito o armaría una fecha imposible). Un primer dígito que no puede ser
/// decena (día 4-9, mes 2-9) se completa con cero.
pub fn mask_type(text: &str, c: char, max: Option<NaiveDate>) -> Option<String> {
    if !c.is_ascii_digit() {
        return None;
    }
    let mut d = digits(text);
    if d.len() >= 6 {
        return None;
    }
    if (d.is_empty() && c > '3') || (d.len() == 2 && c > '1') {
        d.push('0');
    }
    d.push(c);
    valid_prefix(&d, max).then(|| mask_format(&d))
}

/// Borra el último dígito (y con él, su guion).
pub fn mask_backspace(text: &str) -> String {
    let mut d = digits(text);
    d.pop();
    mask_format(&d)
}

/// Pegado de una fecha completa (`05/11/2026`, `5-11-26`…) o de dígitos sueltos.
pub fn mask_paste(pasted: &str, max: Option<NaiveDate>) -> String {
    let parts: Vec<&str> = pasted
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .collect();
    let stream: String = if let [day, month, year] = parts[..] {
        let pad = |s: &str| format!("{s:0>2}");
        let year = if year.len() >= 4 {
            &year[year.len() - 2..]
        } else {
            year
        };
        format!("{}{}{}", pad(day), pad(month), pad(year))
    } else {
        digits(pasted)
    };
    let mut text = String::new();
    for c in stream.chars() {
        match mask_type(&text, c, max) {
            Some(next) => text = next,
            None => break,
        }
    }
    text
}

/// `DD-MM-AA` completo a fecha (el año de dos dígitos es 20AA).
pub fn mask_parse(text: &str) -> Option<NaiveDate> {
    let mut parts = text.trim().split('-');
    let (day, month, year) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || [day, month, year].iter().any(|p| p.len() != 2) {
        return None;
    }
    NaiveDate::from_ymd_opt(
        2000 + year.parse::<i32>().ok()?,
        month.parse().ok()?,
        day.parse().ok()?,
    )
}

pub fn mask_show(date: NaiveDate) -> String {
    date.format("%d-%m-%y").to_string()
}

// --- máscara de hora HH:MM ----------------------------------------------------------

/// `1115` a `11:15`; con la hora completa deja los dos puntos puestos (`11:`).
pub fn time_format(digits: &str) -> String {
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i == 2 {
            out.push(':');
        }
        out.push(c);
    }
    if digits.len() == 2 {
        out.push(':');
    }
    out
}

fn valid_time_prefix(d: &str) -> bool {
    let n = |range: std::ops::Range<usize>| d.get(range).and_then(|s| s.parse::<u32>().ok());
    if !d.is_empty() && n(0..1).is_none_or(|tens| tens > 2) {
        return false;
    }
    if d.len() >= 2 && n(0..2).is_none_or(|hour| hour > 23) {
        return false;
    }
    !(d.len() >= 3 && n(2..3).is_none_or(|tens| tens > 5))
}

/// Escribe un dígito en la máscara `HH:MM`. Un primer dígito 3-9 se completa
/// con cero (`9` es `09:`). `None` = la tecla se rechaza.
pub fn time_type(text: &str, c: char) -> Option<String> {
    if !c.is_ascii_digit() {
        return None;
    }
    let mut d = digits(text);
    if d.len() >= 4 {
        return None;
    }
    if d.is_empty() && c > '2' {
        d.push('0');
    }
    d.push(c);
    valid_time_prefix(&d).then(|| time_format(&d))
}

pub fn time_backspace(text: &str) -> String {
    let mut d = digits(text);
    d.pop();
    time_format(&d)
}

/// Pegado de `10:30`, `9:5` o `1030`.
pub fn time_paste(pasted: &str) -> String {
    let parts: Vec<&str> = pasted
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .collect();
    let stream = if let [hour, minute] = parts[..] {
        format!("{hour:0>2}{minute:0>2}")
    } else {
        digits(pasted)
    };
    let mut text = String::new();
    for c in stream.chars() {
        match time_type(&text, c) {
            Some(next) => text = next,
            None => break,
        }
    }
    text
}

/// `HH:MM` completo a hora.
pub fn time_parse(text: &str) -> Option<NaiveTime> {
    let (hour, minute) = text.trim().split_once(':')?;
    if hour.len() != 2 || minute.len() != 2 {
        return None;
    }
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
}

pub fn time_show(time: NaiveTime) -> String {
    time.format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
    }

    fn typed(max: Option<NaiveDate>, input: &str) -> String {
        let mut text = String::new();
        for c in input.chars() {
            if let Some(next) = mask_type(&text, c, max) {
                text = next;
            }
        }
        text
    }

    #[test]
    fn editing_moves_the_cursor_and_inserts_in_the_middle() {
        let (mut text, mut cursor) = (String::from("hola mundo"), 4);
        assert!(edit(&mut text, &mut cursor, &key(KeyCode::Char('!'))));
        assert_eq!((text.as_str(), cursor), ("hola! mundo", 5));
        edit(&mut text, &mut cursor, &key(KeyCode::Home));
        edit(&mut text, &mut cursor, &key(KeyCode::Delete));
        assert_eq!(text, "ola! mundo");
        edit(&mut text, &mut cursor, &key(KeyCode::End));
        edit(&mut text, &mut cursor, &key(KeyCode::Backspace));
        assert_eq!(text, "ola! mund");
        edit(&mut text, &mut cursor, &key(KeyCode::Left));
        edit(&mut text, &mut cursor, &key(KeyCode::Left));
        edit(&mut text, &mut cursor, &key(KeyCode::Char('ñ')));
        assert_eq!((text.as_str(), cursor), ("ola! muñnd", 8));
        assert!(!edit(&mut text, &mut cursor, &key(KeyCode::Tab)));
        let ctrl = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert!(!edit(&mut text, &mut cursor, &ctrl));
    }

    #[test]
    fn unicode_is_edited_by_characters_not_bytes() {
        let (mut text, mut cursor) = (String::from("añ"), 2);
        edit(&mut text, &mut cursor, &key(KeyCode::Backspace));
        assert_eq!((text.as_str(), cursor), ("a", 1));
    }

    #[test]
    fn paste_drops_control_characters_at_the_cursor() {
        let (mut text, mut cursor) = (String::from("ab"), 1);
        insert_str(&mut text, &mut cursor, "X\nY\u{1b}");
        assert_eq!((text.as_str(), cursor), ("aXYb", 3));
    }

    #[test]
    fn line_view_scrolls_to_keep_the_cursor_visible() {
        let text = "abcdefghij";
        assert_eq!(line_view(text, 3, 5), ("abcde".to_string(), 3));
        assert_eq!(line_view(text, 10, 5), ("ghij".to_string(), 4));
        assert_eq!(line_view(text, 7, 5), ("defgh".to_string(), 4));
        assert_eq!(line_view("", 0, 5), (String::new(), 0));
    }

    #[test]
    fn wrap_breaks_on_words_and_maps_the_cursor() {
        let text = "uno dos tres cuatro";
        let lines = wrap_lines(text, 9);
        let shown: Vec<String> = lines
            .iter()
            .map(|&(s, e)| text.chars().skip(s).take(e - s).collect())
            .collect();
        assert_eq!(shown, ["uno dos ", "tres ", "cuatro"]);
        assert!(lines.iter().all(|&(s, e)| e - s <= 9));
        assert_eq!(cursor_pos(&lines, 0), (0, 0));
        assert_eq!(cursor_pos(&lines, 8), (1, 0));
        assert_eq!(cursor_pos(&lines, text.chars().count()), (2, 6));
        // una palabra más larga que el ancho se corta
        assert_eq!(wrap_lines("abcdefghij", 4), [(0, 4), (4, 8), (8, 10)]);
        assert_eq!(wrap_lines("", 4), [(0, 0)]);
    }

    #[test]
    fn mask_adds_dashes_and_pads_leading_digits() {
        assert_eq!(typed(None, "051126"), "05-11-26");
        assert_eq!(typed(None, "05"), "05-");
        assert_eq!(typed(None, "0511"), "05-11-");
        // 5 de día y 3 de mes se completan con cero
        assert_eq!(typed(None, "53"), "05-03-");
        assert_eq!(typed(None, "5"), "05-");
    }

    #[test]
    fn mask_rejects_letters_and_impossible_days_and_months() {
        assert_eq!(typed(None, "a"), "");
        assert_eq!(typed(None, "32"), "3"); // «32» no es un día: el 2 se rechaza
        assert_eq!(mask_type("3", '2', None), None);
        assert_eq!(mask_type("", '0', None), Some("0".into()));
        assert_eq!(mask_type("0", '0', None), None); // día 00
        assert_eq!(mask_type("05-", '1', None), Some("05-1".into()));
        assert_eq!(mask_type("05-1", '3', None), None); // mes 13
        assert_eq!(mask_type("05-0", '0', None), None); // mes 00
        assert_eq!(mask_type("31-0", '4', None), None); // abril no tiene 31
        assert_eq!(mask_type("30-0", '2', None), None); // febrero no tiene 30
        assert_eq!(mask_type("29-0", '2', None), Some("29-02-".into()));
    }

    #[test]
    fn year_cannot_exceed_the_maximum() {
        let max = Some(today());
        assert_eq!(mask_type("05-11-", '3', max), None); // 3x > 2x
        assert_eq!(mask_type("05-11-2", '7', max), None); // 27 > 26
        assert_eq!(typed(max, "051126"), "05-11-26");
        assert_eq!(typed(max, "051125"), "05-11-25");
        assert_eq!(typed(max, "010200"), "01-02-00");
        // sin tope (deadline) el año es libre
        assert_eq!(typed(None, "051130"), "05-11-30");
    }

    #[test]
    fn future_dates_and_nonexistent_dates_are_refused_on_the_last_digit() {
        let max = Some(today());
        assert_eq!(mask_type("06-11-2", '6', max), None); // mañana
        assert_eq!(mask_type("05-11-2", '6', max), Some("05-11-26".into()));
        assert_eq!(mask_type("01-12-2", '6', max), None); // diciembre aún no llegó
        assert_eq!(mask_type("29-02-2", '5', None), None); // 2025 no es bisiesto
        assert_eq!(mask_type("29-02-2", '4', None), Some("29-02-24".into()));
    }

    #[test]
    fn backspace_removes_the_digit_and_its_dash() {
        assert_eq!(mask_backspace("05-11-26"), "05-11-2");
        assert_eq!(mask_backspace("05-11-2"), "05-11-");
        assert_eq!(mask_backspace("05-11-"), "05-1");
        assert_eq!(mask_backspace("05-"), "0");
        assert_eq!(mask_backspace(""), "");
    }

    #[test]
    fn paste_understands_full_dates_in_several_shapes() {
        let max = Some(today());
        for pasted in ["05/11/2026", "5-11-26", "05.11.26", " 05-11-2026\n"] {
            assert_eq!(mask_paste(pasted, max), "05-11-26", "{pasted:?}");
        }
        assert_eq!(mask_paste("051126", max), "05-11-26");
        // una fecha futura se corta en el último dígito válido
        assert_eq!(mask_paste("06/11/2026", max), "06-11-2");
        assert_eq!(mask_paste("hola", max), "");
    }

    #[test]
    fn parse_requires_a_complete_real_date() {
        assert_eq!(mask_parse("05-11-26"), Some(today()));
        assert_eq!(mask_parse("31-02-26"), None);
        assert_eq!(mask_parse("05-11-"), None);
        assert_eq!(mask_parse("5-11-26"), None);
        assert_eq!(mask_parse("05-11-26-1"), None);
        assert_eq!(mask_show(today()), "05-11-26");
    }

    fn typed_time(input: &str) -> String {
        let mut text = String::new();
        for c in input.chars() {
            if let Some(next) = time_type(&text, c) {
                text = next;
            }
        }
        text
    }

    #[test]
    fn time_mask_adds_the_colon_and_pads_the_first_digit() {
        assert_eq!(typed_time("1115"), "11:15");
        assert_eq!(typed_time("11"), "11:");
        assert_eq!(typed_time("9"), "09:");
        assert_eq!(typed_time("0930"), "09:30");
        assert_eq!(typed_time("x1a0"), "10:");
    }

    #[test]
    fn time_mask_rejects_impossible_hours_and_minutes() {
        assert_eq!(typed_time("24"), "2"); // 24 h no existe
        assert_eq!(typed_time("2359"), "23:59");
        assert_eq!(typed_time("1060"), "10:0"); // el 6 de los minutos se rechaza
        assert_eq!(time_type("10:", '6'), None);
        assert_eq!(time_type("10:", '5'), Some("10:5".into()));
        assert_eq!(time_type("10:30", '1'), None);
    }

    #[test]
    fn time_backspace_paste_and_parse() {
        assert_eq!(time_backspace("11:15"), "11:1");
        assert_eq!(time_backspace("11:"), "1");
        assert_eq!(time_paste("10:30"), "10:30");
        assert_eq!(time_paste("9:5"), "09:05");
        assert_eq!(time_paste("1030"), "10:30");
        assert_eq!(time_paste("25:00"), "2");
        assert_eq!(time_parse("10:30"), NaiveTime::from_hms_opt(10, 30, 0));
        assert_eq!(time_parse("10:3"), None);
        assert_eq!(time_parse("25:00"), None);
        assert_eq!(
            time_show(NaiveTime::from_hms_opt(9, 5, 0).unwrap()),
            "09:05"
        );
    }
}
