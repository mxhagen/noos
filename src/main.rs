use std::path::PathBuf;

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
    match cli::validate(&args) {
        Ok(a) => args = a,
        Err(e) => cli::err_exit(clap::error::ErrorKind::ValueValidation, e),
    };

    logger::init(None, logger::LogLevel::Debug).unwrap();
    debug!("Parsed arguments: {args:?}");

    use cli::{FeedSubcommand, Subcommand};
    match args.command.unwrap_or_default() {
        Subcommand::Feed(cmd) => {
            match cmd {
                FeedSubcommand::Import { file } => {
                    info!("Importing feeds from OPML file: '{file}'");

                    let imported_urls = match data::import_opml_channel_urls(file) {
                        Ok(urls) => urls,
                        Err(e) => {
                            error!("Fatal: Failed to import OPML file: {e}");
                            std::process::exit(1);
                        }
                    };

                    let urls = read_urls_from_channels_file();
                    match data::export_channel_urls_to_config(&urls) {
                        Ok(_) => info!("Successfully updated channels file with imported URLs"),
                        Err(e) => {
                            error!("Failed to update channels file: {e}");
                            std::process::exit(1);
                        }
                    }

                    info!("Imported {} URLs from OPML file", imported_urls.len());
                }

                FeedSubcommand::Export { file } => {
                    info!("Exporting feeds to OPML file: '{file}'");
                    if std::path::PathBuf::from(&file).exists() {
                        error!("Fatal: OPML file '{file}' already exists.",);
                        std::process::exit(1);
                    }

                    let urls = read_urls_from_channels_file();
                    let channels = urls.iter()
                    .flat_map(|url| match data::open_rss_channel(url) {
                        Err(e) => {
                            error!("Failed to open RSS channel at URL '{}': {e}. Skipping channel...", url);
                            None
                        }
                        Ok(c) => Some(c),
                    })
                    .collect::<Vec<_>>();

                    match data::export_opml(file, channels) {
                        Ok(_) => info!("Successfully exported URLs to OPML file"),
                        Err(e) => {
                            error!("Fatal: Failed to export OPML file: {e}");
                            std::process::exit(1);
                        }
                    }

                    info!("Exported {} URLs to OPML file", urls.len());
                }

                FeedSubcommand::List => {
                    let urls = read_urls_from_channels_file();
                    for url in &urls {
                        println!("- {}", url);
                    }
                }

                FeedSubcommand::Add { feed } => {
                    info!("Adding feed URL: '{feed}'");

                    let mut urls = read_urls_from_channels_file();
                    if urls.contains(&feed) {
                        warn!("Feed URL '{feed}' is already in channels file. Skipping...");
                        std::process::exit(0);
                    }

                    urls.push(feed.clone());
                    match data::export_channel_urls_to_config(&urls) {
                        Ok(_) => info!("Successfully added feed URL to channels file"),
                        Err(e) => {
                            error!("Fatal: Failed to update channels file: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                FeedSubcommand::Remove { feed } => {
                    info!("Removing feed URL: '{feed}'");

                    let mut urls = read_urls_from_channels_file();
                    if !urls.contains(&feed) {
                        warn!("Feed URL '{feed}' not found in channels file. Skipping...");
                        std::process::exit(0);
                    }

                    urls.retain(|url| url != &feed);
                    match data::export_channel_urls_to_config(&urls) {
                        Ok(_) => info!("Successfully removed feed URL from channels file"),
                        Err(e) => {
                            error!("Fatal: Failed to update channels file: {e}");
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        Subcommand::Serve { .. } => {
            // TODO: implement web server
            error!("Fatal: The 'serve' subcommand is unimplemented. Use 'dump' for now.");
            std::process::exit(1);
        }

        Subcommand::Dump { file } => {
            let urls = read_urls_from_channels_file();
            info!("Found {} channel URLs in channels file.", urls.len());
            for url in &urls {
                info!("Loading channel from URL: {}", url);
                let channel = get_feed(&url);
                if let Some(ch) = channel {
                    data::add_channel_items(&ch);
                }

                // // to (de-)serialize for testing:
                // info!("Serializing entire sample rss feed to 'sample_feed.bin'...");
                // serialize::save_cache("cache/sample_feed.bin", &serialize::SerdeWrapper(channel)).unwrap();
                //
                // info!("Loading sample rss feed from cache...");
                // let channel = load_feed("cache/sample_feed.bin");
            }

            let (page_template, item_template) =
                load_templates(args.page_template, args.item_template);

            info!("Rendering HTML output...");
            let html = page_template.render((&data::data_store().timeline, &item_template));

            info!("Dumping output HTML to '{}'...", file.display());
            match std::fs::write(file, html) {
                Err(e) => {
                    error!("Fatal: Failed to write output HTML file: {e}");
                    std::process::exit(1);
                }
                Ok(_) => info!("Successfully dumped output HTML file!"),
            }
        }
    }
    // since the above code works, but is hard to read:
    // TODO: Consider moving error handling to frequently called functions instead of bubbling up so much
    //       perhaps just refactor helper/wrapper functions defined in this file back into their own modules

    info!("Success! Exiting...");
}

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

fn load_feed(path: &str) -> rss::Channel {
    let wrapper: serialize::SerdeWrapper<rss::Channel> = serialize::load_cache(path);
    wrapper.0
}

fn print_items(channel: rss::Channel, max: usize) {
    println!("Items from source '{}' (first three)", channel.title());
    for item in channel.items().iter().take(max) {
        println!("Item title: {}", item.title().unwrap_or("No title"));
    }
    println!();
}

fn read_urls_from_channels_file() -> Vec<String> {
    let path = dirs::config_dir()
        .unwrap()
        .join("noos")
        .join("channels.txt");

    if !path.exists() {
        warn!(
            "Channels file '{}' does not exist. Creating an empty one...",
            path.display()
        );

        if let Err(e) = std::fs::create_dir_all(path.parent().unwrap())
            .and_then(|_| std::fs::File::create(&path))
        {
            error!(
                "Failed to create channels file in config directory '{}': {e}.",
                path.display()
            );
            std::process::exit(1);
        }
    }

    let contents = std::fs::read_to_string(&path);
    if let Err(e) = contents {
        error!("Failed to read URLs from file '{}': {e}.", path.display());
        std::process::exit(1);
    }

    contents
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Load HTML templates from cli-args, config or defaults
/// with logging output.
fn load_templates(
    page_template_path: Option<PathBuf>,
    item_template_path: Option<PathBuf>,
) -> (html::PageTemplate, html::ItemTemplate) {
    info!("Parsing HTML templates...");

    let (page_template, item_template) =
        html::load_templates_or_default(page_template_path, item_template_path);

    info!("Finished parsing HTML templates!");
    (page_template, item_template)
}
