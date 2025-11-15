//! An implementation of a simple html template formatter.
//!
//! Provided templates are unchecked -- users are expected to know html,
//! but formatted strings are escaped to prevent injection attacks.

use std::{
    borrow::Cow,
    collections::HashSet,
    path::{Path, PathBuf},
};

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
        for specifier in [
            Title,
            Description,
            Source,
            Link,
            Date,
            Time,
            Timestamp,
            ChannelLink,
        ] {
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

        let (item_title, item_description, item_source, item_link, item_date, item_time, item_timestamp, item_channel_link) = (
            item.title(), item.description(), item.source(), item.link(), item.date(), item.time(), item.timestamp.to_string(),
            item.channel_url.clone()
        );

        // TODO: Refactor item rendering

        use ItemFormatSpecifier::*;
        let (title_encoded, n1) = encode_specifier_with_size(&item_title, Title);
        let (description_encoded, n2) = encode_specifier_with_size(&item_description, Description);
        let (source_encoded, n3) = encode_specifier_with_size(&item_source, Source);
        let (link_encoded, n4) = encode_specifier_with_size(&item_link, Link);
        let (date_encoded, n5) = encode_specifier_with_size(&item_date, Date);
        let (time_encoded, n6) = encode_specifier_with_size(&item_time, Time);
        let (timestamp_encoded, n7) = encode_specifier_with_size(&item_timestamp, Timestamp);
        let (channel_link_encoded, n8) = encode_specifier_with_size(&item_channel_link, ChannelLink);

        for subst in &self.substitutions {
            size += match subst.specifier {
                Title => n1,
                Description => n2,
                Source => n3,
                Link => n4,
                Date => n5,
                Time => n6,
                Timestamp => n7,
                ChannelLink => n8,
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
                Timestamp => &timestamp_encoded,
                ChannelLink => &channel_link_encoded,
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
    type Deps<'a> = (&'a [TimelineItem], &'a ItemTemplate);

    fn parse<S>(template: S) -> Self
    where
        S: ToString,
    {
        let template = template.to_string();
        let mut substitutions = Vec::new();

        use PageFormatSpecifier::*;
        for specifier in [Items, ItemCount, ChannelCount, Date, Time, Timestamp] {
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
        let mut size = self.template.len() as isize;

        let items = content
            .iter()
            .map(|item| item_template.render(item))
            .collect::<String>();

        // Items are already encoded in ItemTemplate::render
        let n1 = items.len() as isize - "${items}".len() as isize;

        let channel_count = content
            .iter()
            .map(|item| &item.channel_url)
            .collect::<HashSet<_>>()
            .len()
            .to_string();

        let (item_count, date, time, timestamp) = (
            content.len().to_string(),
            chrono::Utc::now().format("%Y-%m-%d").to_string(),
            chrono::Utc::now().format("%H:%M:%S").to_string(),
            chrono::Utc::now().timestamp().to_string(),
        );

        use PageFormatSpecifier::*;
        let (item_count_encoded, n2) = encode_specifier_with_size(&item_count, ItemCount);
        let (channel_count_encoded, n3) = encode_specifier_with_size(&channel_count, ChannelCount);
        let (date_encoded, n4) = encode_specifier_with_size(&date, Date);
        let (time_encoded, n5) = encode_specifier_with_size(&time, Time);
        let (timestamp_encoded, n6) = encode_specifier_with_size(&timestamp, Timestamp);

        for subst in &self.substitutions {
            size += match subst.specifier {
                Items => n1,
                ItemCount => n2,
                ChannelCount => n3,
                Date => n4,
                Time => n5,
                Timestamp => n6,
            };
        }

        // Now do the actual rendering with substitutions.
        let mut rendered = String::with_capacity(size as usize);

        // Build the final string
        let mut last_pos = 0;
        for subst in &self.substitutions {
            let (start, end) = (subst.start, subst.end);
            let encoded = match subst.specifier {
                Items => &items.clone().into(),
                ItemCount => &item_count_encoded,
                ChannelCount => &channel_count_encoded,
                Date => &date_encoded,
                Time => &time_encoded,
                Timestamp => &timestamp_encoded,
            };

            rendered.push_str(&self.template[last_pos..start]);
            rendered.push_str(encoded);
            last_pos = end;
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
    // TODO: Reconsider the format specifier escaping logic
    // TODO: Parse all specifiers in one pass/regex for efficiency
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

pub trait Template: Default {
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
    Timestamp,
    ChannelLink,
    // TODO: Add item format specifier for all RSS item fields including media (images)
    //       see https://www.rssboard.org/rss-specification#hrelementsOfLtitemgt
}

/// An enum containing all well-defined
/// format specifiers for page templates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFormatSpecifier {
    Items,
    ItemCount,
    ChannelCount,
    Date,
    Time,
    Timestamp,
    // TODO: Add page format specifier for noos metadata (version/build)
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
            Timestamp => "timestamp",
            ChannelLink => "channel_link",
        };
        write!(f, "{s}")
    }
}

impl std::fmt::Display for PageFormatSpecifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use PageFormatSpecifier::*;
        let s = match self {
            Items => "items",
            ItemCount => "item_count",
            ChannelCount => "channel_count",
            Date => "date",
            Time => "time",
            Timestamp => "timestamp",
        };
        write!(f, "{s}")
    }
}

pub trait FormatSpecifier: std::fmt::Display {}
impl FormatSpecifier for ItemFormatSpecifier {}
impl FormatSpecifier for PageFormatSpecifier {}

// TODO: use serde and build.rs to pre-parse default templates into baked-in binary dump

impl Default for ItemTemplate {
    /// Load and parse the baked-in default item template
    /// NOTE: parsing at runtime is bad, but parsing at comptime is very tedious in rust
    fn default() -> Self {
        let template = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/item.html"));
        Self::parse(template)
    }
}

impl Default for PageTemplate {
    /// Load and parse the baked-in default page template
    /// NOTE: parsing at runtime is bad, but parsing at comptime is very tedious in rust
    fn default() -> Self {
        let template = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/page.html"));
        Self::parse(template)
    }
}

/// Load user-defined templates from config directory,
/// or fall back to the built-in defaults if not found.
pub fn load_templates_or_default<P>(
    page_template_path: Option<P>,
    item_template_path: Option<P>,
) -> (PageTemplate, ItemTemplate)
where
    P: AsRef<Path>,
{
    info!("Parsing HTML templates...");
    let ts = (
        load_template(page_template_path, "page_template.html"),
        load_template(item_template_path, "item_template.html"),
    );
    info!("Finished parsing HTML templates!");

    ts
}

/// Load a template, either using the path specified via cli,
/// or from the user config directory, or the default (in this order)
/// NOTE: use `load_templates_or_default` for loading all templates at once
fn load_template<T, P>(cli_arg: Option<P>, default_name: &str) -> T
where
    T: Template,
    P: AsRef<Path>,
{
    if let Some(path) = cli_arg {
        info!(
            "Using custom template specified in command line arguments: '{}'",
            path.as_ref().display()
        );
        return T::parse_file(path);
    }

    match get_user_config_file(default_name) {
        Some(path) => {
            info!(
                "Using custom template from config directory: '{}'",
                path.display()
            );
            T::parse_file(path)
        }
        None => {
            info!("No custom template found, using default.");
            T::default()
        }
    }
}

/// Get the path of a file in the config directory `$config_dir/noos/$filename`
/// Returns None if the config dir or the file can't be found
/// See `dirs::config_dir` for more info on where this is located
fn get_user_config_file<P: AsRef<Path>>(filename: P) -> Option<PathBuf> {
    let file: PathBuf = dirs::config_dir()?
        .join(env!("CARGO_BIN_NAME"))
        .join(filename);

    file.exists().then_some(file)
}

/// Dump the generated HTML to a file, with logging output.
/// Exits on failure.
pub fn dump_html_to_file<P: AsRef<Path>>(html: &str, path: P) {
    let path = path.as_ref();
    info!("Dumping output HTML to '{}'...", path.display());

    match std::fs::write(path, html) {
        Err(e) => {
            error!("Fatal: Failed to write output HTML file: {e}");
            std::process::exit(1);
        }
        Ok(_) => info!("Successfully dumped output HTML file!"),
    }
}

// TODO: Fix times using UTC instead of local time (everywhere)
//       Use UTC internally, then convert to local for user facing dates/times
