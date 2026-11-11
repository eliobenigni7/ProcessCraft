use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "processcraft-cli", version, about = "Headless ProcessCraft")]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Subcommand)]
pub enum Command {
    LayoutPreview {
        input: PathBuf,
        #[arg(long)]
        document_id: String,
        #[arg(long)]
        diagram_id: String,
        #[arg(long)]
        out: PathBuf,
    },
    LayoutApply {
        input: PathBuf,
        #[arg(long)]
        proposal: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    ExportImage {
        input: PathBuf,
        #[arg(long)]
        document_id: String,
        #[arg(long)]
        diagram_id: String,
        #[arg(long, value_enum)]
        format: ImageFormat,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        #[arg(long)]
        dark: bool,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        acknowledge_diagnostics: bool,
    },
    ImportBpmn {
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    ExportBpmn {
        input: PathBuf,
        #[arg(long)]
        document_id: String,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        acknowledge_native_fields: bool,
    },
    Mcp {
        #[arg(long, conflicts_with_all = ["input", "attach"], required_unless_present_any = ["input", "attach"])]
        new: Option<String>,
        #[arg(long = "in", conflicts_with_all = ["new", "attach"], required_unless_present_any = ["new", "attach"])]
        input: Option<PathBuf>,
        #[arg(long, conflicts_with_all = ["new", "input"], requires_all = ["token", "session"])]
        attach: Option<String>,
        #[arg(long, requires = "attach")]
        token: Option<String>,
        #[arg(long, requires = "attach")]
        session: Option<String>,
        #[arg(long = "read-root")]
        read_roots: Vec<PathBuf>,
        #[arg(long = "write-root")]
        write_roots: Vec<PathBuf>,
    },
    Query {
        path: PathBuf,
        #[command(subcommand)]
        query: Query,
    },
    Info {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Commands {
        #[arg(long)]
        json: bool,
    },
    Validate {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Run {
        #[arg(
            long,
            conflicts_with = "input",
            required_unless_present = "input",
            value_name = "NAME"
        )]
        new: Option<String>,
        #[arg(long = "in", conflicts_with = "new", required_unless_present = "new")]
        input: Option<PathBuf>,
        #[arg(long)]
        batch: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ImageFormat {
    Svg,
    Png,
    Pdf,
}

#[derive(Subcommand)]
pub enum Query {
    /// Read event roots, references, flags and owned data as bounded field pages.
    Events {
        document_id: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Read DI attributes and representation endpoints as bounded field pages.
    Diagrams {
        document_id: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Read persistent semantic/DI selectors and their optional XML IDs.
    Identities {
        document_id: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Read scoped data declarations, ports and associations as bounded field pages.
    Data {
        document_id: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Read supporting catalog values as bounded JSON Pointer pages.
    Catalog {
        document_id: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    Comments {
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    Checkpoints {
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    CompareCheckpoint {
        checkpoint_id: String,
    },
    Library {
        #[arg(default_value = "")]
        text: String,
    },
    Dictionary,
    Usages {
        entry_id: String,
    },
    Table {
        document_id: String,
        process_id: String,
    },
}
