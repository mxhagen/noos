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

/// A shorthand for `Substitution<PageFormatSpecifier>`
type PageSubst = Substitution<PageFormatSpecifier>;
/// A shorthand for `Substitution<ItemFormatSpecifier>`
type ItemSubst = Substitution<ItemFormatSpecifier>;

/// A minimally pre-parsed page template, that allows to
/// calculate positions for substitutions only once.
#[derive(Debug)]
pub struct PageTemplate {
    template: String,
    substitutions: Vec<PageSubst>,
}

/// A minimally pre-parsed item template, that allows to
/// calculate positions for substitutions only once.
#[derive(Debug)]
pub struct ItemTemplate {
    template: String,
    substitutions: Vec<ItemSubst>,
}

impl Template for ItemTemplate {
    type Deps<'a> = &'a TimelineItem;

    fn parse<S>(template: S) -> Self
    where
        S: ToString,
    {
        let template = template.to_string();
        let mut substitutions = Vec::new();

        use ItemFormatSpecifier::*;
        for specifier in [Title, Description, Source, Link, Date, Time] {
            substitutions.extend(
                find_format_specifiers(&template, specifier)
                    .into_iter()
                    .map(|(start, end)| Substitution {
                        start,
                        end,
                        specifier,
                    }),
            );
        }

        substitutions.sort_by_key(|s| s.start);

        Self {
            template: template.to_string(),
            substitutions,
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

    #[rustfmt::skip]
    fn render<'a>(&self, item: Self::Deps<'a>) -> String {
        // Made efficient by using size calculations.
        // Start with template size, then for each substitution,
        // add the size of the encoded string and subtract
        // the size of the format specifier.
        let mut size = self.template.len() as isize;

        let (item_title, item_description, item_source, item_link, item_date, item_time) = (
            item.title(), item.description(), item.source(), item.link(), item.date(), item.time()
        );

        use ItemFormatSpecifier::*;
        let (title_encoded, n1) = encode_specifier_with_size(&item_title, Title);
        let (description_encoded, n2) = encode_specifier_with_size(&item_description, Description);
        let (source_encoded, n3) = encode_specifier_with_size(&item_source, Source);
        let (link_encoded, n4) = encode_specifier_with_size(&item_link, Link);
        let (date_encoded, n5) = encode_specifier_with_size(&item_date, Date);
        let (time_encoded, n6) = encode_specifier_with_size(&item_time, Time);

        for subst in &self.substitutions {
            size += match subst.specifier {
                Title => n1,
                Description => n2,
                Source => n3,
                Link => n4,
                Date => n5,
                Time => n6,
            };
        }

        // Now do the actual rendering with substitutions.
        let mut rendered = String::with_capacity(size as usize);

        // Build the final string
        let mut last_pos = 0;
        for subst in &self.substitutions {
            let (start, end) = (subst.start, subst.end);
            let encoded = match subst.specifier {
                Title => &title_encoded,
                Description => &description_encoded,
                Source => &source_encoded,
                Link => &link_encoded,
                Date => &date_encoded,
                Time => &time_encoded,
            };

            rendered.push_str(&self.template[last_pos..start]);
            rendered.push_str(encoded);
            last_pos = end;
        }
        rendered.push_str(&self.template[last_pos..]);

        rendered
    }
}

impl Template for PageTemplate {
    type Deps<'a> = (&'a BTreeMap<i64, TimelineItem>, &'a ItemTemplate);

    fn parse<S>(template: S) -> Self
    where
        S: ToString,
    {
        let template = template.to_string();
        let mut substitutions = find_format_specifiers(&template, PageFormatSpecifier::Items)
            .into_iter()
            .map(|(start, end)| Substitution {
                start,
                end,
                specifier: PageFormatSpecifier::Items,
            })
            .collect::<Vec<PageSubst>>();

        substitutions.sort_by_key(|s| s.start);

        Self {
            template: template.to_string(),
            substitutions,
        }
    }

    /// NOTE: Exits on file read error, see logging output.
    fn parse_file<P: AsRef<std::path::Path>>(path: P) -> Self {
        let template = std::fs::read_to_string(path).unwrap_or_else(|e| {
            error!("Failed to read template file: {e}");
            error!("Exiting...");
            std::process::exit(1);
        });

        Self::parse(template)
    }

    fn render<'a>(&self, (content, item_template): Self::Deps<'a>) -> String {
        if self.substitutions.is_empty() {
            warn!(
                "No substitutions found in page template -- Your page will not contain any items!"
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
                    - PageFormatSpecifier::Items.to_string().len() as isize)
                    * self.substitutions.len() as isize) as usize,
        );

        let mut last_pos = 0;
        for subst in &self.substitutions {
            rendered.push_str(&self.template[last_pos..subst.start]);
            rendered.push_str(&items_string);
            last_pos = subst.end;
        }
        rendered.push_str(&self.template[last_pos..]);

        rendered
    }
}

/// Find the positions of all occurrences of a format specifier in a template.
/// Format specifiers are of the form `${specifier}`,
/// and can be escaped (ignored) with a leading backslash `\`.
fn find_format_specifiers<F>(template: &str, specifier: F) -> Vec<(usize, usize)>
where
    F: FormatSpecifier,
{
    let re = format!(r"(?:^|[^\\])\$\{{{specifier}\}}");
    let re = Regex::new(&re).unwrap();

    let specifier = specifier.to_string();
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
        positions.push((start, end));
    }

    if positions.is_empty() {
        debug!("Format specifier '${{{specifier}}}' not found in template");
    }

    positions
}

/// Helper to get html encoded string (Cow) and its size for a given specifier.
fn encode_specifier_with_size<'a, F: FormatSpecifier>(
    s: &'a str,
    specifier: F,
) -> (Cow<'a, str>, isize) {
    let encoded = encode_safe(s);
    let n = encoded.len() as isize;
    (
        encoded,
        n - "${}".len() as isize - specifier.to_string().len() as isize,
    )
}

pub trait Template {
    /// A type representing dependencies required for rendering
    type Deps<'a>
    where
        Self: 'a;

    /// Parse a template from a string for efficient rendering
    fn parse<S>(template: S) -> Self
    where
        S: ToString;

    /// Parse a template from a string for efficient rendering
    fn parse_file<P: AsRef<std::path::Path>>(path: P) -> Self;

    /// Render the template with given dependencies
    fn render<'a>(&self, content: Self::Deps<'a>) -> String;
}

/// A position of a format specifier in a template string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Substitution<F: FormatSpecifier> {
    start: usize,
    end: usize,
    specifier: F,
}

/// An enum containing all well-defined
/// format specifiers for item templates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemFormatSpecifier {
    Title,
    Description,
    Source,
    Link,
    Date,
    Time,
    // TODO: Add item format specifier for timestamp (beside string date/time)
    // TODO: Add item format specifier for channel url (not article url)
    // TODO: Add item format specifier for all RSS item fields including media (images)
    //       see https://www.rssboard.org/rss-specification#hrelementsOfLtitemgt
}

/// An enum containing all well-defined
/// format specifiers for page templates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFormatSpecifier {
    Items,
    // TODO: Add page format specifier for item count
    // TODO: Add page format specifier for source count
    // TODO: Add page format specifier for update date/time (strings + timestamp)
    // TODO: Add page format specifier for noos metadata (build)
}

impl std::fmt::Display for ItemFormatSpecifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ItemFormatSpecifier::*;
        let s = match self {
            Title => "title",
            Description => "description",
            Source => "source",
            Link => "link",
            Date => "date",
            Time => "time",
        };
        write!(f, "{s}")
    }
}

impl std::fmt::Display for PageFormatSpecifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use PageFormatSpecifier::*;
        let s = match self {
            Items => "items",
        };
        write!(f, "{s}")
    }
}

pub trait FormatSpecifier: std::fmt::Display {}
impl FormatSpecifier for ItemFormatSpecifier {}
impl FormatSpecifier for PageFormatSpecifier {}
