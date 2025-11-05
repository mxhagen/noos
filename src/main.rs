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

    info!("Parsing HTML templates...");
    let page_template = html::PageTemplate::parse_file("templates/page.html");
    let item_template = html::ItemTemplate::parse_file("templates/item.html");

    // // Fetch and store a sample rss feed
    //
    // let channel = get_feed();
    //
    // info!("Serializing entire sample rss feed to 'sample_feed.bin'...");
    // serialize::save_cache("cache/sample_feed.bin", &serialize::SerdeWrapper(channel)).unwrap();

    // // Load and print the sample rss feed from cache
    // info!("Loading sample rss feed from cache...");
    // let channel = load_feed("cache/sample_feed.bin");
    // data::add_channel_items(&channel);

    // Load a few channels from channels.txt
    let urls = read_urls_from_file("channels.txt");
    info!("Found {} channel URLs in 'channels.txt'", urls.len());
    for url in urls {
        info!("Loading channel from URL: {}", url);
        let channel = get_feed(&url);
        if let Some(ch) = channel {
            data::add_channel_items(&ch);
        }
    }

    info!("Rendering HTML output...");
    let html = page_template.render((&data::data_store().timeline, &item_template));

    info!("Writing output HTML to 'output.html'...");
    std::fs::write("output.html", html).expect("Failed to write output HTML file");

    // print_items(channel, 3);

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

fn read_urls_from_file(path: &str) -> Vec<String> {
    let contents = std::fs::read_to_string(path);
    if let Err(e) = contents {
        error!("Failed to read URLs from file '{path}': {e}.");
        return Vec::new();
    }

    contents
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}
