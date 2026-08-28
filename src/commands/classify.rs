use clap::{Arg, ArgAction, ArgMatches, Command};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

pub fn subcommand() -> Command {
    Command::new("classify")
        .about("Classify files into directories based on their extensions")
        .arg(
            Arg::new("directory")
                .help("The directory to classify")
                .required(true),
        )
        .arg(
            Arg::new("extensions")
                .short('e')
                .long("extensions")
                .help("Custom directory names for extensions (e.g., 'raf:RAW')")
                .num_args(1..)
                .value_parser(clap::builder::ValueParser::string()),
        )
        .arg(
            Arg::new("ignore")
                .long("ignore")
                .help("Extensions to ignore (e.g., 'heic' or 'all')")
                .num_args(1..)
                .value_parser(clap::builder::ValueParser::string()),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help("Recursively classify files in subdirectories")
                .action(ArgAction::SetTrue),
        )
}

pub fn run(matches: &ArgMatches) {
    let target_dir = PathBuf::from(matches.get_one::<String>("directory").unwrap());

    let mut extension_map: HashMap<String, String> = HashMap::new();
    if let Some(exts) = matches.get_many::<String>("extensions") {
        for ext in exts {
            if let Some((key, value)) = ext.split_once(':') {
                extension_map.insert(format!(".{}", key.to_lowercase()), value.to_string());
            }
        }
    }

    let mut ignored_extensions: HashSet<String> = HashSet::new();
    if let Some(ignores) = matches.get_many::<String>("ignore") {
        for ext in ignores {
            ignored_extensions.insert(format!(".{}", ext.to_lowercase()));
        }
    }

    sort_directory(
        &target_dir,
        &extension_map,
        &ignored_extensions,
        matches.get_flag("recursive"),
    );
}

fn sort_directory(
    dir: &Path,
    ext_map: &HashMap<String, String>,
    ignored_exts: &HashSet<String>,
    recursive: bool,
) {
    match fs::read_dir(dir) {
        Ok(entries) => {
            let mut move_map: Vec<(PathBuf, String, String)> = Vec::new();
            let mut subdirectories: Vec<PathBuf> = Vec::new();
            let mut extensions: HashSet<String> = HashSet::new();

            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    subdirectories.push(path);
                    continue;
                }

                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let file_ext = format!(".{}", ext.to_lowercase());
                    extensions.insert(file_ext.clone());

                    let target_dir_name = ext_map.get(&file_ext).cloned();

                    if let Some(sub_dir) = target_dir_name {
                        move_map.push((
                            dir.to_path_buf(),
                            entry.file_name().to_string_lossy().into(),
                            sub_dir,
                        ));
                    } else if !ignored_exts.contains(&file_ext) && !ignored_exts.contains(".all") {
                        move_map.push((
                            dir.to_path_buf(),
                            entry.file_name().to_string_lossy().into(),
                            ext.to_uppercase(),
                        ));
                    }
                }
            }

            if extensions.len() > 1 {
                for (base_dir, file_name, sub_dir) in move_map {
                    move_file(&base_dir, &file_name, &sub_dir);
                }
            }

            if recursive {
                for subdir in subdirectories {
                    sort_directory(&subdir, ext_map, ignored_exts, recursive);
                }
            }
        }
        Err(err) => eprintln!("Error reading directory {:?}: {}", dir, err),
    }
}

fn move_file(base_dir: &Path, file_name: &str, target_sub_dir: &str) {
    let target_dir = base_dir.join(target_sub_dir);
    let old_path = base_dir.join(file_name);
    let new_path = target_dir.join(file_name);

    if let Err(err) = fs::create_dir_all(&target_dir) {
        eprintln!("Error creating directory {:?}: {}", target_dir, err);
        return;
    }

    if let Err(err) = fs::rename(&old_path, &new_path) {
        eprintln!("Error moving file {:?}: {}", file_name, err);
    } else {
        println!("Moved {} to {}", file_name, target_sub_dir);
    }
}
