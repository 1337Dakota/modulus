use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use color_eyre::Result;
use directories::ProjectDirs;
use inquire::{Select, Text};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

macro_rules! ignore_cancellation {
    ($stmt:expr) => {{
        let result = $stmt; // Execute the statement
        match result {
            Ok(v) => Ok(v), // Return Ok value
            Err(inquire::InquireError::OperationCanceled) => return Ok(()), // Handle OperationCancelled
            Err(inquire::InquireError::OperationInterrupted) => return Ok(()), // Handle OperationInterrupted
            _ => result, // For other errors, return the original result
        }
    }};
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Template {
    name: String,
    ignored_files: HashSet<String>,
    variables: HashMap<String, String>,
    #[serde(skip)]
    folder_path: PathBuf,
}

fn copy_directory(src: &Path, dest: &Path) -> Result<()> {
    // Create the destination directory
    if !dest.exists() {
        fs::create_dir_all(dest)?;
    }

    // Walk through the source directory
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let dest_path = dest.join(entry.path().strip_prefix(src)?);

        if entry.file_type().is_dir() {
            // Create directories
            fs::create_dir_all(dest_path)?;
        } else {
            // Copy files
            fs::copy(entry.path(), dest_path)?;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let mut templates: HashMap<String, Template> = HashMap::new();

    let project_dir = ProjectDirs::from("", "", "modulus")
        .expect("Could not find a suitable configuration directory");
    let config_dir = std::env::var("MODULUS_CONFIG_DIR")
        .map_or(project_dir.config_dir().to_path_buf(), |path| {
            PathBuf::from(path)
        });
    fs::create_dir_all(config_dir.clone()).expect("Could not create configuration directory");

    if fs::read_dir(config_dir.clone()).unwrap().next().is_none() {
        println!("No templates loaded!");
        println!("Insert Templates into {}", config_dir.to_string_lossy());
        return Ok(());
    }

    for template in WalkDir::new(config_dir).max_depth(1).min_depth(1) {
        let template = match template {
            Ok(v) => v,
            Err(_) => continue,
        };

        if template.file_type().is_dir() {
            let path = template.into_path();
            let template_id = match path.file_name() {
                Some(v) => v,
                None => continue,
            }
            .to_string_lossy()
            .to_string();
            let meta_file = path.join(format!("{}.meta.toml", template_id));
            if !meta_file.exists() {
                eprintln!("No Metafile found for template at {:#?}", path);
                continue;
            }
            let mut template: Template = toml::from_str(&std::fs::read_to_string(meta_file)?)?;
            template
                .ignored_files
                .insert(format!("{}.meta.toml", template_id));
            template.folder_path = path;
            templates.insert(template.name.clone(), template);
        }
    }

    let selected_template_name: &String =
        ignore_cancellation!(Select::new("Template", templates.keys().collect()).prompt())?;

    let selected_template = templates.get(selected_template_name).unwrap();

    let destination: PathBuf = PathBuf::from(ignore_cancellation!(Text::new("Destination")
        .with_default(".")
        .prompt())?);

    let mut variables: HashMap<String, String> = HashMap::new();
    for (variable, prompt) in selected_template.variables.clone() {
        variables.insert(
            variable,
            Text::new(&prompt)
                .prompt()
                .expect("Operation interrupted, Destination is unfinished"),
        );
    }

    std::fs::create_dir_all(destination.clone()).expect("Could not create destination path");
    copy_directory(&selected_template.folder_path, &destination)
        .expect("Could not copy template to destination");

    for entry in WalkDir::new(destination.clone()) {
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_file() {
            println!("{:#?}; {:#?}", entry.path(), selected_template.folder_path);

            if selected_template.ignored_files.contains(
                &entry
                    .path()
                    .strip_prefix(destination.clone())
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            ) {
                continue;
            };
            if entry
                .path()
                .parent()
                .filter(|x| {
                    selected_template
                        .ignored_files
                        .contains(&x.file_name().unwrap().to_string_lossy().to_string())
                })
                .is_some()
            {
                continue;
            }
            let mut content = fs::read_to_string(entry.path()).expect("Could not read file");
            for (variable, value) in &variables {
                let mut new_content = String::new();
                let mut start = 0;
                let to_find = format!("<{}>", variable.to_lowercase());

                while let Some(pos) = content.to_lowercase()[start..].find(&to_find)
                // Find the position (case insensitive)
                {
                    // Append the part before the found match
                    new_content.push_str(&content[start..pos]);
                    // Append the replacement string
                    new_content.push_str(value);
                    // Update the end of the last match
                    start = pos + to_find.len();
                }

                new_content.push_str(&content[start..]);

                content = new_content
            }
            fs::write(entry.path(), content).expect("Could not write to file");
        }
    }

    Ok(())
}
