use std::path::Path;

use clap::Parser;

mod cli;
mod data;
mod html;
mod logger;
mod serialize;

pub use logger::LogLevel;

use html::Template;

fn main() {
    // Arg-parsing and initialization
    let mut args = cli::Args::parse();
    args = cli::validate(&args);

    logger::init(None, args.verbosity).unwrap();
    debug!("Parsed arguments: {args:?}");

    use cli::{FeedSubcommand, Subcommand};
    match args.clone().command.unwrap_or_default() {
        Subcommand::Serve { .. } => serve_handler(),
        Subcommand::Dump { file } => dump_handler(file, &args),
        Subcommand::Feed(cmd) => match cmd {
            FeedSubcommand::Import { file } => import_handler(&file),
            FeedSubcommand::Export { file } => export_handler(&file),
            FeedSubcommand::List => list_handler(),
            FeedSubcommand::Add { feed } => add_handler(feed),
            FeedSubcommand::Remove { feed } => remove_handler(feed),
        },
    }

    info!("Success! Exiting...");
}

/// Dump aggregated feed items to static HTML file
fn dump_handler<P: AsRef<Path>>(file: P, args: &cli::Args) {
    let urls = data::read_urls_from_config_channels_file();
    info!("Found {} channel URLs in channels file.", urls.len());
    for url in &urls {
        info!("Loading channel from URL: {}", url);
        let channel = get_feed(&url);
        if let Some(ch) = channel {
            data::add_channel_items(&ch);
        }
    }

    let (page_template, item_template) =
        html::load_templates_or_default(args.page_template.clone(), args.item_template.clone());

    let html = page_template.render((&data::data_store().timeline, &item_template));

    html::dump_html_to_file(&html, file);
}

/// Start web server to serve aggregated feed items
/// Currently unimplemented -- just errs and exits
fn serve_handler() {
    // TODO: implement web server
    error!("Fatal: The 'serve' subcommand is unimplemented. Use 'dump' for now.");
    std::process::exit(1);
}

/// Import OPML, merge with existing channels, and export to channels file
fn import_handler(file: &str) {
    // Get urls to import from OPML file
    let mut urls = data::import_opml_channel_urls(file);

    // Also read existing urls from channels file
    urls.extend(data::read_urls_from_config_channels_file());

    // Write all urls to channels file
    data::export_channel_urls_to_config(&urls);
}

/// Export channels from channels file to OPML
fn export_handler(file: &str) {
    info!("Exporting feeds to OPML file: '{file}'");
    if std::path::PathBuf::from(&file).exists() {
        error!("Fatal: OPML file '{file}' already exists.",);
        std::process::exit(1);
    }

    let urls = data::read_urls_from_config_channels_file();
    let channels = data::open_rss_channels(&urls);

    data::export_opml(file, channels);

    info!(
        "Exported {} URLs from channels file to OPML file",
        urls.len()
    );
}

/// List all feed URLs in channels file
fn list_handler() {
    data::read_urls_from_config_channels_file()
        .iter()
        .for_each(|url| println!("{url}"));
}

/// Add a feed URL to channels file
fn add_handler(feed: String) {
    info!("Adding feed URL: '{feed}'");

    let mut urls = data::read_urls_from_config_channels_file();
    if urls.contains(&feed) {
        warn!("Feed URL '{feed}' is already in channels file. Skipping...");
        std::process::exit(0);
    }

    urls.push(feed);
    data::export_channel_urls_to_config(&urls);
}

/// Remove a feed URL from channels file
fn remove_handler(feed: String) {
    info!("Removing feed URL: '{feed}'");

    let mut urls = data::read_urls_from_config_channels_file();
    if !urls.contains(&feed) {
        warn!("Feed URL '{feed}' not found in channels file. Skipping...");
        std::process::exit(0);
    }

    urls.retain(|url| url != &feed);
    data::export_channel_urls_to_config(&urls);
}

/// Fetch and parse an RSS feed from a URL
fn get_feed(url: &str) -> Option<rss::Channel> {
    // Get a sample rss feed
    match data::open_rss_channel(url) {
        Err(e) => {
            error!("Failed to open RSS channel: {e}. Skipping channel...");
            None
        }
        Ok(c) => c.into(),
    }
}
