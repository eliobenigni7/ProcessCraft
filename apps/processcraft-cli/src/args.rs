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
    /// Propose an auto-layout for one diagram and write it as a proposal JSON file.
    LayoutPreview {
        /// Input .processcraft project.
        input: PathBuf,
        /// Document ID (listed by `info`).
        #[arg(long)]
        document_id: String,
        /// Diagram ID within the document (listed by `info`).
        #[arg(long)]
        diagram_id: String,
        /// Lay out the direct children of this expanded SubChoreography representation.
        #[arg(long)]
        choreography_scope_shape: Option<String>,
        /// Proposal JSON file to write.
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Apply a layout proposal to its project and save the result to a new file.
    LayoutApply {
        /// Input .processcraft project.
        input: PathBuf,
        /// Proposal JSON written by layout-preview.
        #[arg(long)]
        proposal: PathBuf,
        /// New .processcraft file to write.
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Render one diagram to SVG, PNG or PDF.
    ExportImage {
        /// Input .processcraft project.
        input: PathBuf,
        /// Document ID (listed by `info`).
        #[arg(long)]
        document_id: String,
        /// Diagram ID within the document (listed by `info`).
        #[arg(long)]
        diagram_id: String,
        /// Image format.
        #[arg(long, value_enum)]
        format: ImageFormat,
        /// Image file to write.
        #[arg(long)]
        out: PathBuf,
        /// Scale factor within 0.125..8.
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Render with the dark appearance.
        #[arg(long)]
        dark: bool,
        /// Replace the output file if it exists.
        #[arg(long)]
        overwrite: bool,
        /// Export even when the render preflight reports diagnostics.
        #[arg(long)]
        acknowledge_diagnostics: bool,
    },
    /// Import BPMN 2.0 XML into a new .processcraft project (editable when it fits the qualified profile).
    ImportBpmn {
        /// BPMN XML file.
        input: PathBuf,
        /// .processcraft file to write.
        #[arg(long)]
        out: PathBuf,
        /// Replace the output file if it exists.
        #[arg(long)]
        overwrite: bool,
    },
    /// Export one document of a project as BPMN 2.0 XML.
    ExportBpmn {
        /// Input .processcraft project.
        input: PathBuf,
        /// Document ID (listed by `info`).
        #[arg(long)]
        document_id: String,
        /// BPMN XML file to write.
        #[arg(long)]
        out: PathBuf,
        /// Replace the output file if it exists.
        #[arg(long)]
        overwrite: bool,
        /// Export even though native-only project links are omitted.
        #[arg(long)]
        acknowledge_native_fields: bool,
    },
    /// Serve the Model Context Protocol over stdio, standalone or attached to the GUI.
    Mcp {
        /// Start from a new, unsaved project with this name.
        #[arg(long, conflicts_with_all = ["input", "attach"], required_unless_present_any = ["input", "attach"])]
        new: Option<String>,
        /// Open this .processcraft project.
        #[arg(long = "in", conflicts_with_all = ["new", "attach"], required_unless_present_any = ["new", "attach"])]
        input: Option<PathBuf>,
        /// Attach to the GUI session published on this named pipe.
        #[arg(long, conflicts_with_all = ["new", "input"], requires_all = ["token", "session"])]
        attach: Option<String>,
        /// Attachment token shown by the GUI.
        #[arg(long, requires = "attach")]
        token: Option<String>,
        /// GUI session ID to attach to.
        #[arg(long, requires = "attach")]
        session: Option<String>,
        /// Folder MCP file tools may read from (repeatable).
        #[arg(long = "read-root")]
        read_roots: Vec<PathBuf>,
        /// Folder MCP file tools may write to (repeatable).
        #[arg(long = "write-root")]
        write_roots: Vec<PathBuf>,
    },
    /// Read project projections as JSON without modifying the file.
    Query {
        /// Input .processcraft project.
        path: PathBuf,
        #[command(subcommand)]
        query: Query,
    },
    /// Summarize a project or BPMN file: document and diagram IDs, import outcome.
    Info {
        /// .processcraft project or BPMN XML file.
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// Emit JSON (always the output format).
        #[arg(long)]
        json: bool,
    },
    /// List command descriptors and the JSON Schema of the request protocol.
    Commands {
        /// Emit JSON (always the output format).
        #[arg(long)]
        json: bool,
    },
    /// Validate a project, or the prepared import of a BPMN file.
    Validate {
        /// .processcraft project or BPMN XML file.
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// Emit JSON (always the output format).
        #[arg(long)]
        json: bool,
    },
    /// Apply a command batch to a new or existing project and save it to --out.
    Run {
        /// Start from a new project with this name.
        #[arg(
            long,
            conflicts_with = "input",
            required_unless_present = "input",
            value_name = "NAME"
        )]
        new: Option<String>,
        /// Start from this .processcraft project.
        #[arg(long = "in", conflicts_with = "new", required_unless_present = "new")]
        input: Option<PathBuf>,
        /// JSON batch request file (schema: `commands --json`).
        #[arg(long)]
        batch: PathBuf,
        /// .processcraft file to write.
        #[arg(long)]
        out: PathBuf,
        /// Replace an existing output file other than the input.
        #[arg(long)]
        overwrite: bool,
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
    /// Read every modeled document field, including ordered XML and common metadata.
    Model {
        document_id: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Read detailed checkpoint differences as bounded JSON Pointer pages.
    CheckpointFields {
        checkpoint_id: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
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
