use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{atomic::AtomicU16, Arc},
};

mod md;

pub fn compile() {
    fs::remove_dir_all("public").unwrap_or_default();
    fs::create_dir_all("public").unwrap();
    fs::create_dir_all("include").unwrap();
    fs::create_dir_all("embed").unwrap();
    copy_dir("include", "public").unwrap();

    let mut embed = HashMap::new();
    embedded_files(&mut embed, "embed");
    let embed = Arc::new(embed);

    let threads_to_wait = Arc::new(AtomicU16::new(0));
    for path in fs::read_dir("include").unwrap() {
        md::folder(path.unwrap().path(), threads_to_wait.clone(), embed.clone())
    }
    while threads_to_wait.load(std::sync::atomic::Ordering::Relaxed) > 0 {
        std::thread::sleep(std::time::Duration::from_millis(1))
    }
}

pub fn embedded_files(hm: &mut HashMap<String, Vec<String>>, root: impl AsRef<Path>) {
    let entries = fs::read_dir(root).unwrap();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            embedded_files(hm, path);
        } else {
            let data = fs::read_to_string(&path).unwrap();
            let content: Vec<&str> = data.split('\n').collect();
            let name = path
                .strip_prefix("embed")
                .unwrap()
                .with_extension("")
                .to_str()
                .unwrap()
                .to_string();
            let mut lines = Vec::with_capacity(content.len());
            for c in &content {
                lines.push(c.to_string());
            }
            hm.insert(name, lines);
        }
    }
}

pub fn copy_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else if let Some(ext) = entry.path().extension() {
            if ext != "md" {
                fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
            }
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
