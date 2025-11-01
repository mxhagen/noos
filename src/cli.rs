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
    #[arg(short = 'v', long = "verbosity", value_name = "0-3", default_value_t = LogLevel::Info, verbatim_doc_comment)]
    pub verbosity: LogLevel,
    // TODO: Make templates specifiable via CLI
    // /// Path to the html template for item/article rendering
    // #[arg(long = "item-template")]
    // pub item_template: Option<std::path::PathBuf>,
    //
    // /// Path to the html template for the page surrounding the articles
    // #[arg(long = "page-template")]
    // pub page_template: Option<std::path::PathBuf>,
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

    // TODO: implement dump subcommand
    // /// Dump the rendered html of the web interface to a file
    // #[command(alias = "d")]
    // Dump {
    //     /// File to write the dumped HTML to
    //     #[arg(short = 'f', long = "file", default_value = "dump.html")]
    //     file: std::path::PathBuf,
    // },
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
    // TODO: OPML support
    // /// Import all feeds from an OPML file
    // Import { file: String },
    // /// Export all feeds to an OPML file
    // Export { file: String },
}

/// Semantically validate and process cli arguments
pub fn validate(args: &Args) -> Result<Args, String> {
    let mut validated = args.clone();

    if args.command.is_none() {
        validated.command = Some(Subcommand::Serve {
            port: 9005,
            bind: "127.0.0.1".to_string(),
            open: true,
        });
    }

    Ok(validated)
}

/// Shorthand for `Args::command().error(...).exit()`
pub fn err_exit(kind: clap::error::ErrorKind, message: impl std::fmt::Display) {
    // TODO: Reconcile this with the logger at some point
    Args::command().error(kind, message).exit()
}

// TODO: Add config file support
