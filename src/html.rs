//! An implementation of a simple html template formatter.
//!
//! Provided templates are unchecked -- users are expected to know html,
//! but formatted strings are escaped to prevent injection attacks.

use std::{borrow::Cow, collections::BTreeMap};

use html_escape::encode_safe;
use regex::Regex;

use crate::data::TimelineItem;

#[allow(unused_imports)]
use crate::{debug, error, info, log, warn};

/// A minimally pre-parsed page template, that allows to
/// calculate positions for substitutions only once.
#[derive(Debug)]
pub struct PageTemplate {
    template: String,
    items_pos: Vec<Position>,
}

/// A minimally pre-parsed item template, that allows to
/// calculate positions for substitutions only once.
#[derive(Debug)]
pub struct ItemTemplate {
    // TODO: Refactor: single sorted `Vec<(Position, Specifier)>` to avoid resorting in render
    template: String,
    title_pos: Vec<Position>,
    description_pos: Vec<Position>,
    source_pos: Vec<Position>,
    link_pos: Vec<Position>,
    date_pos: Vec<Position>,
    time_pos: Vec<Position>,
}

impl Template<TimelineItem> for ItemTemplate {
    /// Parse an item template to make substitutions efficient.
    fn parse<S>(template: S) -> Self
    where
        S: ToString,
    {
        let template = template.to_string();

        use format_specifiers::*;

        let title_pos = find_format_specifiers(&template, TITLE);
        let description_pos = find_format_specifiers(&template, DESCRIPTION);
        let source_pos = find_format_specifiers(&template, SOURCE);
        let link_pos = find_format_specifiers(&template, LINK);
        let date_pos = find_format_specifiers(&template, DATE);
        let time_pos = find_format_specifiers(&template, TIME);

        Self {
            template: template.to_string(),
            title_pos,
            description_pos,
            source_pos,
            link_pos,
            date_pos,
            time_pos,
        }
    }

    fn parse_file<P: AsRef<std::path::Path>>(path: P) -> Self {
        let template = std::fs::read_to_string(path).unwrap_or_else(|e| {
            error!("Failed to read template file: {e}");
            error!("Exiting...");
            std::process::exit(1);
        });

        Self::parse(template)
    }
}

impl ItemTemplate {
    #[rustfmt::skip]
    pub fn render(&self, item: &TimelineItem) -> String {
        // Made efficient by using size calculations.
        // Start with template size, then for each substitution,
        // add the size of the encoded string and subtract
        // the size of the format specifier.
        let mut size = self.template.len() as isize;

        /// Helper to get encoded string (Cow) and its size.
        fn encoded_with_size<'a>(s: &'a str, specifier: &str) -> (Cow<'a, str>, isize) {
            let encoded = encode_safe(s);
            let n = encoded.len() as isize;
            (encoded, n - "${}".len() as isize - specifier.len() as isize)
        }

        let (item_title, item_description, item_source, item_link, item_date, item_time) = (
            item.title(), item.description(), item.source(), item.link(), item.date(), item.time()
        );

        use format_specifiers::*;
        let (title_encoded, n1) = encoded_with_size(&item_title, TITLE);
        let (description_encoded, n2) = encoded_with_size(&item_description, DESCRIPTION);
        let (source_encoded, n3) = encoded_with_size(&item_source, SOURCE);
        let (link_encoded, n4) = encoded_with_size(&item_link, LINK);
        let (date_encoded, n5) = encoded_with_size(&item_date, DATE);
        let (time_encoded, n6) = encoded_with_size(&item_time, TIME);

        size += n1 * self.title_pos.len() as isize + n2 * self.description_pos.len() as isize
            + n3 * self.source_pos.len() as isize  + n4 * self.link_pos.len() as isize
            + n5 * self.date_pos.len() as isize    + n6 * self.time_pos.len() as isize;

        // Now do the actual rendering with substitutions.
        let mut rendered = String::with_capacity(size as usize);
        let mut replacements: Vec<(&Position, _)> =
            (self.title_pos.iter().map(|p| (p, title_encoded.clone())))
                .chain(self.description_pos.iter().map(|p| (p, description_encoded.clone())))
                .chain(self.source_pos.iter().map(|p| (p, source_encoded.clone())))
                .chain(self.link_pos.iter().map(|p| (p, link_encoded.clone())))
                .chain(self.date_pos.iter().map(|p| (p, date_encoded.clone())))
                .chain(self.time_pos.iter().map(|p| (p, time_encoded.clone())))
                .collect();

        // Sort replacements by position
        replacements.sort_by_key(|r| r.0.start);

        // Build the final string
        let mut last_pos = 0;
        for (pos, encoded) in replacements {
            rendered.push_str(&self.template[last_pos..pos.start]);
            rendered.push_str(&encoded);
            last_pos = pos.end;
        }
        rendered.push_str(&self.template[last_pos..]);

        rendered
    }
}

impl Template<BTreeMap<i64, TimelineItem>> for PageTemplate {
    /// Parse a page template to make substitutions efficient.
    fn parse<S>(template: S) -> Self
    where
        S: ToString,
    {
        let template = template.to_string();

        let mut items_pos = find_format_specifiers(&template, format_specifiers::ITEMS);
        items_pos.sort_by_key(|p| p.start);

        Self {
            template: template.to_string(),
            items_pos,
        }
    }

    /// Parse a page template from a file at a given path.
    /// NOTE: Exits on file read error, see logging output.
    fn parse_file<P: AsRef<std::path::Path>>(path: P) -> Self {
        let template = std::fs::read_to_string(path).unwrap_or_else(|e| {
            error!("Failed to read template file: {e}");
            error!("Exiting...");
            std::process::exit(1);
        });

        Self::parse(template)
    }
}

impl PageTemplate {
    /// Render a page, by rendering all items and substituting them into the page template.
    pub fn render(
        &self,
        content: &BTreeMap<i64, TimelineItem>,
        item_template: &ItemTemplate,
    ) -> String {
        if self.items_pos.is_empty() {
            warn!(
                "No items position found in page template -- Your page will not contain any items!"
            );
            return self.template.clone();
        }

        // String of all rendered items
        // Recent items first (rev), excluding items dated in the future (filter).
        let items_string = content
            .iter()
            .rev()
            .filter_map(|(ts, item)| (chrono::Utc::now().timestamp() >= *ts).then_some(item))
            .map(|item| item_template.render(item))
            .collect::<String>();

        // Now do the actual rendering with substitutions.
        let mut rendered = String::with_capacity(
            (self.template.len() as isize
                + (items_string.len() as isize
                    - "${}".len() as isize
                    - format_specifiers::ITEMS.len() as isize)
                    * self.items_pos.len() as isize) as usize,
        );

        let mut last_pos = 0;
        for pos in self.items_pos.clone() {
            rendered.push_str(&self.template[last_pos..pos.start]);
            rendered.push_str(&items_string);
            last_pos = pos.end;
        }
        rendered.push_str(&self.template[last_pos..]);

        rendered
    }
}

/// Find the positions of all occurrences of a format specifier in a template.
/// Format specifiers are of the form `${specifier}`,
/// and can be escaped (ignored) with a leading backslash `\`.
fn find_format_specifiers(template: &str, specifier: &str) -> Vec<Position> {
    let re = format!(r"(?:^|[^\\])\$\{{{specifier}\}}");
    let re = Regex::new(&re).unwrap();

    let mut positions = Vec::new();

    for m in re.find_iter(template) {
        let start = if m.start() == 0 { 0 } else { m.start() + 1 }; // account for leading non-backslash char
        // Extra safety: ignore if escaped
        if start > 0 && template.as_bytes()[start.saturating_sub(1)] == b'\\' {
            debug!("Format specifier '${{{specifier}}}' is escaped, ignoring");
            continue;
        }
        let end = start + specifier.len() + "${}".len();
        debug!("Found format specifier '${{{specifier}}}' at position: ({start:?}-{end:?})");
        positions.push(Position { start, end });
    }

    if positions.is_empty() {
        debug!("Format specifier '${{{specifier}}}' not found in template");
    }

    positions
}

pub trait Template<Content> {
    fn parse<S>(template: S) -> Self
    where
        S: ToString;

    fn parse_file<P: AsRef<std::path::Path>>(path: P) -> Self;

    // TODO: make html-rendering generic over templates using a `Deps` type?

    // Takes different content types and optionally references
    // nested templates, therefore not generic over Content, but
    // implemented for all Templates.
    // fn render(&self, content: &Content) -> String;
}

/// A Position of a format specifier in a template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Position {
    start: usize,
    end: usize,
}

/// Defined format specifiers for templates.
mod format_specifiers {
    pub const ITEMS: &str = "items";

    pub const TITLE: &str = "title";
    pub const DESCRIPTION: &str = "description";
    pub const SOURCE: &str = "source";
    pub const LINK: &str = "link";
    pub const DATE: &str = "date";
    pub const TIME: &str = "time";
}
