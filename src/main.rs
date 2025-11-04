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

    // // Fetch and store a sample rss feed
    //
    // let channel = get_feed();
    //
    // info!("Serializing entire sample rss feed to 'sample_feed.bin'...");
    // serialize::save_cache("cache/sample_feed.bin", &serialize::SerdeWrapper(channel)).unwrap();

    // Load and print the sample rss feed from cache
    info!("Loading sample rss feed from cache...");
    let channel = load_feed("cache/sample_feed.bin");
    data::add_channel_items(&channel);

    info!("Parsing HTML templates...");
    let page_template = html::PageTemplate::parse_file("templates/page.html");
    let item_template = html::ItemTemplate::parse_file("templates/item.html");

    info!("Rendering HTML output...");
    let html = page_template.render((&data::data_store().timeline, &item_template));

    info!("Writing output HTML to 'output.html'...");
    std::fs::write("output.html", html).expect("Failed to write output HTML file");

    // print_items(channel, 3);

    info!("Success! Exiting...");
}

fn get_feed() -> rss::Channel {
    // Get a sample rss feed
    let url = "https://rss.nytimes.com/services/xml/rss/nyt/HomePage.xml";
    match data::open_rss_channel(url) {
        Err(e) => {
            error!("Failed to open RSS channel: {e}");
            error!("Exiting...");
            std::process::exit(1);
        }
        Ok(c) => c,
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
