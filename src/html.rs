//! An implementation of a simple html template formatter.
//!
//! Provided templates are unchecked -- users are expected to know html,
//! but formatted strings are escaped to prevent injection attacks.

use std::collections::BTreeMap;

use html_escape::encode_safe;
use regex::Regex;

use crate::data::TimelineItem;

#[allow(unused_imports)]
use crate::{debug, error, info, log, warn};

/// A minimally pre-parsed page template, that allows to
/// calculate positions for substitutions only once.
pub struct PageTemplate {
    template: String,
    items_pos: Option<Position>,
}

/// A minimally pre-parsed item template, that allows to
/// calculate positions for substitutions only once.
pub struct ItemTemplate {
    template: String,
    // TODO: Support multiple occurrences of the same format specifier (maybe switch to strfmt?)
    title_pos: Option<Position>,
    description_pos: Option<Position>,
    source_pos: Option<Position>,
    link_pos: Option<Position>,
    date_pos: Option<Position>,
    time_pos: Option<Position>,
}

impl Template<TimelineItem> for ItemTemplate {
    /// Parse an item template to make substitutions efficient.
    /// NOTE: Exits on invalid template, see logging output.
    fn parse<S>(template: S) -> Self
    where
        S: ToString,
    {
        let template = template.to_string();

        /// Helper to find positions of format specifiers with error handling.
        fn helper(template: &str, specifier: &str) -> Position {
            position_or_exit(
                find_format_specifier(template, specifier),
                &format!("format specifier '${{{specifier}}}'"),
            )
        }

        use format_specifiers::*;

        let title_pos = helper(&template, TITLE).into();
        let description_pos = helper(&template, DESCRIPTION).into();
        let source_pos = helper(&template, SOURCE).into();
        let link_pos = helper(&template, LINK).into();
        let date_pos = helper(&template, DATE).into();
        let time_pos = helper(&template, TIME).into();

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
        // We use some simple size calculations to avoid reallocs.
        // For all format specifiers in the template that DO OCCUR,
        // we add the length of the escaped content and subtract
        // the length of the format specifier itself.
        let size = self.template.len();

        let helper = |specifier: &str, content: &str| {
            let escaped = encode_safe(content).to_string();
            let n = escaped.len() - (specifier.len() + "${}".len());
            (escaped, size + n)
        };

        use format_specifiers::*;
        let (title, size) = self.title_pos
            .map_or(("(No title)".into(), 0), |_| helper(TITLE, &item.title()));

        let (description, size) = self.description_pos
            .map_or(("(No description)".into(), size), |_| {
                helper(DESCRIPTION, &item.description())
            });

        let (source, size) = self.source_pos.map_or(("(No source)".into(), size), |_| {
            helper(SOURCE, &item.source())
        });

        let (link, size) = self.link_pos
            .map_or(("".into(), size), |_| helper(LINK, &item.link()));

        let (date, size) = self.date_pos
            .map_or(("".into(), size), |_| helper(DATE, &item.date()));

        let (time, size) = self.time_pos
            .map_or(("".into(), size), |_| helper(TIME, &item.time()));

        // Now build the complete string
        let mut html = String::with_capacity(size);
        let mut last_pos = 0;

        let mut insertions: Vec<_> = [
            self.link_pos.map(|p| (p, link)),
            self.title_pos.map(|p| (p, title)),
            self.description_pos.map(|p| (p, description)),
            self.source_pos.map(|p| (p, source)),
            self.date_pos.map(|p| (p, date)),
            self.time_pos.map(|p| (p, time)),
        ].into_iter().flatten().collect();

        insertions.sort_by_key(|(pos, _)| pos.start);

        for (pos, content) in insertions {
            html.push_str(&self.template[last_pos..pos.start]);
            html.push_str(&content);
            last_pos = pos.end;
        }

        html.push_str(&self.template[last_pos..]);
        html
    }
}

impl Template<BTreeMap<i64, TimelineItem>> for PageTemplate {
    /// Parse a page template to make substitutions efficient.
    /// NOTE: Exits on invalid template, see logging output.
    fn parse<S>(template: S) -> Self
    where
        S: ToString,
    {
        let template = template.to_string();

        let items_pos = find_format_specifier(&template, format_specifiers::ITEMS);

        Self {
            template: template.to_string(),
            items_pos,
        }
    }

    /// Parse a page template from a file at a given path.
    /// NOTE: Exits on invalid template or file read error, see logging output.
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
        if self.items_pos.is_none() {
            warn!("No items position found in page template");
            return self.template.clone();
        }

        let items_pos = self.items_pos.unwrap();

        let items = content
            .iter()
            // Larger timestamps (later articles) first
            .rev()
            // Skip items dated in the future
            .filter_map(
                |(&ts, item)| match chrono::Utc::now().timestamp_millis() < ts {
                    true => None,
                    false => Some(item),
                },
            )
            .map(|item| item_template.render(item))
            .collect::<String>();

        let mut rendered = String::with_capacity(self.template.len() + items.len());
        rendered.push_str(&self.template[..items_pos.start]);
        rendered.push_str(&items);
        rendered.push_str(&self.template[items_pos.end..]);
        rendered
    }
}

/// Find the position of a format specifier in a template.
/// Format specifiers are of the form `${specifier}`,
/// and can be escaped (ignored) with a leading backslash `\`.
fn find_format_specifier(template: &str, specifier: &str) -> Option<Position> {
    let re = format!(r"(?:^|[^\\])\$\{{{specifier}\}}");
    let re = Regex::new(&re).unwrap();

    let start = re.find(template).map(|m| match m.start() {
        0 => 0,
        n => n + 1, // account for leading non-backslash char
    });

    if start.is_none() {
        debug!("Format specifier '${{{specifier}}}' not found in template");
        return None;
    }

    let start = start.unwrap();
    if template.as_bytes()[start] == b'\\' {
        debug!("Format specifier '${{{specifier}}}' is escaped, ignoring");
        return None;
    }

    let end = start + specifier.len() + "${}".len();
    debug!("Found format specifier '${{{specifier}}}' at position: ({start:?}-{end:?})");

    Position { start, end }.into()
}

/// Helper for common pattern of exiting on missing position.
fn position_or_exit(pos: Option<Position>, description: &str) -> Position {
    match pos {
        Some(p) => p,
        None => {
            error!("Failed to find {description} in template");
            error!("Exiting...");
            std::process::exit(1);
        }
    }
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
