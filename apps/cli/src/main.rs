use capture_core::{Annotation, Command, Document, Kind, Session};
use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf};

#[derive(Parser)]
#[command(about = "Local deterministic screenshot document tools")]
struct Args {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Styles {
        #[command(subcommand)]
        command: StylesAction,
    },
    History {
        #[command(subcommand)]
        command: HistoryAction,
    },
    New {
        #[arg(long, default_value_t = 960)]
        width: u32,
        #[arg(long, default_value_t = 600)]
        height: u32,
        /// Canvas color as RRGGBBAA (use 00000000 for transparency).
        #[arg(long, default_value = "ffffffff")]
        background: String,
        #[arg(long)]
        out: PathBuf,
    },
    Import {
        image: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Inspect {
        project: PathBuf,
    },
    Render {
        project: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Export PNG/JPEG/PDF through the same engine as the editor.
    Export {
        project: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 92)]
        quality: u8,
        #[arg(long)]
        paginate: bool,
        #[arg(long)]
        a4: bool,
        #[arg(long, default_value_t = 36.0)]
        margin: f64,
    },
    /// Render a bounded display viewport without opening an application window.
    RenderViewport {
        project: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 1024)]
        width: u32,
        #[arg(long, default_value_t = 768)]
        height: u32,
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        #[arg(long, default_value_t = 0.0)]
        x: f64,
        #[arg(long, default_value_t = 0.0)]
        y: f64,
    },
    Apply {
        project: PathBuf,
        #[arg(long)]
        commands: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Demo {
        #[arg(long)]
        out_dir: PathBuf,
    },
    Compare {
        expected: PathBuf,
        actual: PathBuf,
    },
}
#[derive(Subcommand)]
enum StylesAction {
    List {
        directory: PathBuf,
    },
    Save {
        directory: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        color: String,
        #[arg(long)]
        width: u32,
    },
    Remove {
        directory: PathBuf,
        name: String,
    },
    Apply {
        project: PathBuf,
        #[arg(long)]
        directory: Option<PathBuf>,
        #[arg(long)]
        name: String,
        #[arg(long)]
        id: u64,
        #[arg(long)]
        out: PathBuf,
    },
}
#[derive(Subcommand)]
enum HistoryAction {
    List {
        directory: PathBuf,
        #[arg(long, default_value = "")]
        query: String,
    },
    Record {
        directory: PathBuf,
        project: PathBuf,
        #[arg(long, default_value = "Project snapshot")]
        label: String,
    },
    Open {
        directory: PathBuf,
        id: String,
        #[arg(long)]
        out: PathBuf,
    },
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Action::Styles { command } => match command {
            StylesAction::List { directory } => println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"presets":capture_core::styles::presets(),"saved":capture_core::styles::Store::open(&directory)?.entries()})
                )?
            ),
            StylesAction::Save {
                directory,
                name,
                color,
                width,
            } => capture_core::styles::Store::open(&directory)?.save(
                capture_core::styles::ArrowStyle {
                    shadow: None,
                    line_style: None,
                    name,
                    color: parse_rgba(&color)?,
                    width,
                },
            )?,
            StylesAction::Remove { directory, name } => {
                capture_core::styles::Store::open(&directory)?.remove(&name)?
            }
            StylesAction::Apply {
                project,
                directory,
                name,
                id,
                out,
            } => {
                let mut styles = if let Some(directory) = directory {
                    capture_core::styles::Store::open(&directory)?
                        .entries()
                        .to_vec()
                } else {
                    vec![]
                };
                styles.extend(capture_core::styles::presets());
                let style = styles
                    .iter()
                    .find(|s| s.name.to_lowercase() == name.to_lowercase())
                    .ok_or("Unknown arrow style")?;
                let mut session = Session::new(Document::load(&project)?)?;
                let annotation = session
                    .document
                    .annotations
                    .iter()
                    .find(|a| a.id == id)
                    .ok_or("Annotation does not exist")?;
                if let Some(command) = style.command(annotation)? {
                    session.execute(command)?;
                }
                session.document.save(&out)?;
            }
        },
        Action::History { command } => match command {
            HistoryAction::List { directory, query } => println!(
                "{}",
                serde_json::to_string_pretty(
                    &capture_core::history::History::open(&directory)?
                        .entries()
                        .iter()
                        .filter(|entry| entry.matches_query(&query))
                        .collect::<Vec<_>>()
                )?
            ),
            HistoryAction::Record {
                directory,
                project,
                label,
            } => {
                let entry = capture_core::history::History::open(&directory)?.record(
                    &Document::load(&project)?,
                    &label,
                    capture_core::history::Origin::Project,
                )?;
                println!("{}", serde_json::to_string(&entry)?);
            }
            HistoryAction::Open { directory, id, out } => {
                capture_core::history::History::open(&directory)?
                    .load(&id)?
                    .save(&out)?
            }
        },
        Action::New {
            width,
            height,
            background,
            out,
        } => Document::canvas(width, height, parse_rgba(&background)?)?.save(&out)?,
        Action::Import { image, out } => Document::open_image(&image)?.save(&out)?,
        Action::Inspect { project } => {
            let d = Document::load(&project)?;
            println!(
                "{}",
                serde_json::json!({"version":d.version,"width":d.width,"height":d.height,"annotations":d.annotations,"has_source_image":!d.source_png.is_empty()})
            );
        }
        Action::Export {
            project,
            out,
            quality,
            paginate,
            a4,
            margin,
        } => {
            let format = capture_core::export::Format::from_path(&out)?;
            let mut options = capture_core::export::Options {
                format,
                quality,
                ..Default::default()
            };
            options.page.paginate = paginate;
            options.page.margin = margin;
            if a4 {
                options.page.size = [595.276, 841.89];
            }
            capture_core::export::save(&Document::load(&project)?, &out, &options)?;
        }
        Action::Render { project, out } => Document::load(&project)?.export_png(&out)?,
        Action::RenderViewport {
            project,
            out,
            width,
            height,
            scale,
            x,
            y,
        } => {
            Document::load(&project)?
                .display_list()?
                .render_viewport(capture_core::display::Viewport {
                    size: [width, height],
                    origin: [x, y],
                    scale,
                })?
                .save(out)?;
        }
        Action::Apply {
            project,
            commands,
            out,
        } => {
            let commands: Vec<Command> = serde_json::from_slice(&fs::read(commands)?)?;
            let mut session = Session::new(Document::load(&project)?)?;
            for command in commands {
                session.execute(command)?;
            }
            session.document.save(&out)?;
        }
        Action::Demo { out_dir } => {
            fs::create_dir_all(&out_dir)?;
            let mut base =
                image::RgbaImage::from_pixel(960, 600, image::Rgba([245, 247, 250, 255]));
            for (x, y, pixel) in base.enumerate_pixels_mut() {
                if y < 65 {
                    *pixel = image::Rgba([30, 43, 62, 255]);
                } else if x < 190 {
                    *pixel = image::Rgba([224, 231, 240, 255]);
                } else if (230..900).contains(&x) && (115..450).contains(&y) {
                    *pixel = image::Rgba([255, 255, 255, 255]);
                }
                if (270..550).contains(&x) && (175..205).contains(&y) {
                    *pixel = image::Rgba([60, 110, 190, 255]);
                }
                if (270..800).contains(&x) && (240..255).contains(&y) {
                    *pixel = image::Rgba([205, 213, 224, 255]);
                }
            }
            base.save(out_dir.join("source.png"))?;
            let mut s = Session::new(Document::from_image(base)?)?;
            for a in [
                Annotation {
                    image: None,
                    step_outline: None,
                    line_style: None,
                    shadow: None,
                    focus: None,
                    highlighter_rect: false,
                    group: None,
                    rotation_degrees: 0.0,
                    mirror_x: false,
                    id: 1,
                    kind: Kind::Arrow,
                    start: [680.0, 130.0],
                    end: [550.0, 180.0],
                    color: [240, 85, 55, 255],
                    width: 4,
                    text: None,
                    corner_radius: 0.0,
                    fill: None,
                    points: Vec::new(),
                    callout: None,
                },
                Annotation {
                    image: None,
                    step_outline: None,
                    line_style: None,
                    shadow: None,
                    focus: None,
                    highlighter_rect: false,
                    group: None,
                    rotation_degrees: 0.0,
                    mirror_x: false,
                    id: 2,
                    kind: Kind::Rectangle,
                    start: [250.0, 155.0],
                    end: [570.0, 220.0],
                    color: [240, 85, 55, 255],
                    width: 3,
                    text: None,
                    corner_radius: 0.0,
                    fill: None,
                    points: Vec::new(),
                    callout: None,
                },
            ] {
                s.execute(Command::Add { annotation: a })?;
            }
            s.document.save(&out_dir.join("demo.iclip"))?;
            s.document.export_png(&out_dir.join("demo.png"))?;
            println!(
                "Synthetic fixture and editable project written to {}",
                out_dir.display()
            );
        }
        Action::Compare { expected, actual } => {
            let a = Document::open_image(&expected)?.render()?;
            let b = Document::open_image(&actual)?.render()?;
            if a.dimensions() != b.dimensions() {
                return Err("Image dimensions differ".into());
            }
            let changed = a.pixels().zip(b.pixels()).filter(|(a, b)| a != b).count();
            println!(
                "{}",
                serde_json::json!({"changed_pixels":changed,"total_pixels":u64::from(a.width())*u64::from(a.height()),"exact_match":changed==0})
            );
            if changed != 0 {
                std::process::exit(2);
            }
        }
    }
    Ok(())
}

fn parse_rgba(value: &str) -> Result<[u8; 4], String> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 8 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Background must be eight hexadecimal digits: RRGGBBAA".into());
    }
    let mut color = [0; 4];
    for (index, channel) in color.iter_mut().enumerate() {
        *channel =
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(color)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_accepts_rgba_and_rejects_malformed_input() {
        assert_eq!(parse_rgba("#1234AB80").unwrap(), [18, 52, 171, 128]);
        assert_eq!(parse_rgba("00000000").unwrap(), [0, 0, 0, 0]);
        for bad in ["fff", "fffffffff", "zzzzzzzz", "12345678x", "é345678"] {
            assert!(parse_rgba(bad).is_err());
        }
    }
}
