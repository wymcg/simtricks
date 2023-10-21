use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about=None)]
pub(crate) struct SimtricksArgs {
    /// Width of the matrix, in number of LEDs
    #[arg(short = 'x', long)]
    pub width: usize,

    /// Height of the matrix, in number of LEDs
    #[arg(short = 'y', long)]
    pub height: usize,

    /// Path to plugin
    #[arg(short, long)]
    pub path: String,

    /// Number of frames per second at which to simulate the matrix
    #[arg(short, long, default_value = "30")]
    pub fps: f64,

    /// Add a host that the plugin may connect to
    #[arg(long)]
    pub allow_host: Option<Vec<String>>,

    /// Map a path on the local filesystem to the plugin filesystem, as a pair of paths seperated by a greater than symbol (i.e. "LOCAL_PATH>PLUGIN_PATH")
    #[arg(long)]
    pub map_path: Option<Vec<String>>,
}
