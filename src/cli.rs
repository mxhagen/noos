//! Command line interface parsing and validation

use crate::LogLevel;
use clap::*;

/// A pragmatic RSS aggregator with a browser interface and no built-in reader.
#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Args {
    /// Subcommand to execute. Defaults to starting the web server if none provided.
    #[command(subcommand)]
    pub command: Option<Subcommand>,

    /// Set the minimum level for all logged messages
    /// Accepted values in ascending verbosity are:
    /// - "error", "warn", "info", "debug" (case insensitive)
    /// - or 0-3 (where 0 = Error, 1 = Warn, 2 = Info, 3 = Debug)
    // TODO: change default verbosity to Info once stable
    #[arg(short = 'v', long = "verbosity", value_name = "0-3", default_value_t = LogLevel::Debug, verbatim_doc_comment)]
    pub verbosity: LogLevel,

    /// Path to the html template for item/article rendering
    #[arg(long = "item-template")]
    pub item_template: Option<std::path::PathBuf>,

    /// Path to the html template for the page surrounding the articles
    #[arg(long = "page-template")]
    pub page_template: Option<std::path::PathBuf>,
    // TODO: cli option for timelining strategy (fallback timestamps)
    //       options could be: default to now-1min, discard item, or:
    //       "sprinkle" (evenly distribute articles with missing timestamps between other articles)
}

#[derive(Subcommand, Debug, Clone)]
pub enum Subcommand {
    /// Start the web server
    #[command(alias = "s")]
    Serve {
        /// Port to serve on
        #[arg(short = 'p', long = "port", default_value_t = 9005)]
        port: u16,

        /// Address to bind to
        #[arg(short = 'b', long = "bind", default_value = "127.0.0.1")]
        bind: String,

        /// Open the web interface in the default browser
        #[arg(short = 'o', long = "open", default_value_t = true)]
        open: bool,
    },

    /// Dump the rendered html of the web interface to a file
    #[command(alias = "d")]
    Dump {
        /// File to write the dumped HTML to
        #[arg(short = 'f', long = "file", default_value = "noos.html")]
        file: std::path::PathBuf,
    },
    /// Manage individual feeds
    #[command(subcommand)]
    Feed(FeedSubcommand),
}

#[derive(Subcommand, Debug, Clone)]
pub enum FeedSubcommand {
    /// List all subscribed feeds
    List,
    /// Add a new feed by URL
    Add { feed: String },
    /// Remove a feed by URL
    Remove { feed: String },
    /// Import all feeds from an OPML file. Note: see `$config_dir/noos/channels.txt`
    Import { file: String },
    /// Export all feeds to an OPML file. Note: see `$config_dir/noos/channels.txt`
    Export { file: String },
}

/// Semantically validate and process cli arguments
/// Exits on failure
pub fn validate(args: &Args) -> Args {
    args.clone() // No proper validation needed just yet
}

impl Default for Subcommand {
    /// Default to dumping the rendered HTML to "noos.html"
    fn default() -> Self {
        Subcommand::Dump {
            file: "noos.html".into(),
        }
        // TODO: Set default subcommand to serve once server is implemented
        // Subcommand::Serve {
        //     port: 9005,
        //     bind: "127.0.0.1".into(),
        //     open: true,
        // }
    }
}

// TODO: Add config file support
