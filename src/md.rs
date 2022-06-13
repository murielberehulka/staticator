use std::{
    collections::HashMap,
    fmt, fs,
    io::Write,
    path::{self, Path, PathBuf},
    sync::{atomic::AtomicU16, Arc},
};

pub const TAB_SIZE: usize = 4;

struct Element {
    pub name: Option<String>,
    pub level: usize,
    pub attrs: String,
    pub content: Option<String>,
    pub children: Vec<Element>,
    pub self_close: bool,
}

impl fmt::Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut tabs = Vec::with_capacity(self.level);
        for _ in 0..self.level {
            tabs.push(b'\t');
        }
        let tabs = String::from_utf8_lossy(&tabs);
        match &self.name {
            Some(name) => {
                let has_children = self.children.len() > 0;
                let has_content = self.content.is_some();
                if has_children {
                    if has_content {
                        write!(
                            f,
                            "{0}<{1}{2}>\n{0}    {3}\n",
                            tabs,
                            name,
                            self.attrs,
                            self.content.as_ref().unwrap().to_string()
                        )
                        .unwrap();
                    } else {
                        write!(f, "{0}<{1}{2}>\n", tabs, name, self.attrs,).unwrap();
                    }
                } else {
                    if has_content {
                        write!(
                            f,
                            "{}<{}{}>{}",
                            tabs,
                            name,
                            self.attrs,
                            self.content.as_ref().unwrap()
                        )
                        .unwrap();
                    } else if self.self_close {
                        write!(f, "{}<{}{}", tabs, name, self.attrs,).unwrap();
                    } else {
                        write!(f, "{}<{}{}>", tabs, name, self.attrs).unwrap();
                    }
                }
                for c in &self.children {
                    write!(f, "{}\n", c).unwrap();
                }
                if has_content {
                    if has_children {
                        write!(f, "{}</{}>", tabs, name).unwrap();
                    } else {
                        write!(f, "</{}>", name).unwrap();
                    }
                } else {
                    if has_children {
                        write!(f, "{}</{}>", tabs, name).unwrap();
                    } else if self.self_close {
                        write!(f, "/>").unwrap();
                    } else {
                        write!(f, "</{}>", name).unwrap();
                    }
                }
            }
            None => {
                write!(f, "{}{}", tabs, self.content.as_ref().unwrap()).unwrap();
            }
        }
        Ok(())
    }
}

pub fn folder(
    path: PathBuf,
    threads_to_wait: Arc<AtomicU16>,
    embed: Arc<HashMap<String, Vec<String>>>,
) {
    if path.is_dir() {
        for path in path.read_dir().unwrap() {
            folder(path.unwrap().path(), threads_to_wait.clone(), embed.clone())
        }
    } else {
        if path.extension().unwrap().to_string_lossy() == "md" {
            threads_to_wait.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let threads_to_wait_clone = threads_to_wait.clone();
            std::thread::spawn(move || {
                file(path, embed.clone());
                threads_to_wait_clone.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            });
        }
    }
}

fn parse(lines: Vec<String>) -> Vec<Element> {
    let mut e = vec![];
    let mut padding = 0;
    for line in lines {
        //println!("'{}'", line);
        if line.len() == 0 {
            continue;
        }
        let mut l = line.len();
        let line = line.trim_start();
        l -= line.len();
        if e.len() == 2 {
            l += 4;
        }

        let mut self_close = true;
        let words: Vec<&str> = line.split(' ').collect();
        if words.len() < 1 {
            continue;
        }
        let mut classes = vec![];
        let name = if words[0].starts_with('>') {
            match words[0] {
                ">code_padding_right" => {
                    padding += 1;
                    continue;
                }
                ">code_padding_left" => {
                    padding -= 1;
                    continue;
                }
                _ => {}
            };
            let name = words[0].trim_start_matches('>').to_string();
            if name.starts_with(".") {
                classes.push(name.strip_prefix(".").unwrap().to_string());
                self_close = false;
                Some("div".to_string())
            } else {
                if name == "div" {
                    self_close = false;
                }
                Some(name)
            }
        } else {
            None
        };
        let mut wi = match name {
            Some(_) => 1,
            None => 0,
        };
        let ws = words.len();
        let mut contents = Vec::new();
        let mut attrs = Vec::new();
        while wi < ws {
            let mut values = Vec::new();
            let mut close_after_quote = false;
            while wi < ws {
                if words[wi].starts_with("roc=") {
                    let mut new = words[wi].replace("roc=", "onclick=\"window.location='");
                    new.push_str("'\"");
                    attrs.push(new);
                    break;
                }
                values.push(words[wi].to_string());
                if close_after_quote && words[wi].contains("\"") {
                    break;
                } else if words[wi].contains("=\"") {
                    close_after_quote = true;
                    if words[wi].matches('"').count() > 1 {
                        break;
                    }
                }
                wi += 1;
            }
            //println!("values: '{:?}'", values);
            let values = values.join(" ");
            if close_after_quote {
                attrs.push(values);
                //println!("attr");
            } else {
                //println!("content");
                contents.push(values);
            }
            wi += 1;
        }
        if classes.len() > 0 {
            attrs.push(format!("class=\"{}\"", classes.join(",")).to_string());
        }
        let element = Element {
            name,
            level: (l / 4) + padding,
            attrs: if attrs.len() > 0 {
                ([String::from(" "), attrs.join(" ")]).into_iter().collect()
            } else {
                String::new()
            },
            content: if contents.len() > 0 {
                Some(contents.join(" "))
            } else {
                None
            },
            children: vec![],
            self_close,
        };
        if l == 0 {
            e.push(element);
        } else {
            get_element_in_level(&mut e, (l / TAB_SIZE) + padding)
                .children
                .push(element);
        }
    }
    e
}

pub fn file(path: PathBuf, embed: Arc<HashMap<String, Vec<String>>>) {
    let mut variables = HashMap::<String, String>::new();
    let data = fs::read_to_string(&path).unwrap();
    let mut lines = vec![];
    let raw_lines: Vec<&str> = data.split('\n').collect();

    let rll = raw_lines.len();
    let mut rli = 0;
    while rli < rll {
        let line = &raw_lines[rli];
        if line.starts_with("for_each_folder") {
            let path: Vec<&str> = line.split("=").collect();
            let path = path[1];
            let mut folders = vec![];
            match fs::read_dir(path) {
                Ok(dir) => {
                    for entry in dir {
                        let entry = entry.unwrap();
                        let path = entry.path();
                        if path.is_dir() {
                            let settings_file = fs::read_to_string(
                                Path::new(&path.display().to_string()).join(Path::new("env.conf")),
                            );
                            let mut settings = HashMap::new();
                            settings.insert(
                                "url".to_string(),
                                path.strip_prefix("include").unwrap().display().to_string(),
                            );
                            if let Ok(settings_file) = settings_file {
                                for line in settings_file.split('\n') {
                                    let args: Vec<&str> = line.split("=").collect();
                                    if args.len() != 2 {
                                        continue;
                                    }
                                    settings.insert(args[0].to_string(), args[1].to_string());
                                }
                            }
                            folders.push(settings);
                        }
                    }
                }
                Err(e) => panic!("Error reading dir {}: {}", path, e),
            }
            let srli = rli + 1;
            for folder in folders {
                rli = srli;
                while raw_lines[rli] != ";;;" {
                    let mut line = raw_lines[rli].to_string();
                    let mut print_line = true;
                    // for_each_md_in_current_folder
                    if line == "for_each_md_in_current_folder" {
                        print_line = false;
                        let current_folder =
                            Path::new("include/").join(Path::new(folder.get("url").unwrap()));
                        let files_raw = match fs::read_dir(&current_folder) {
                            Ok(v) => v,
                            Err(e) => {
                                panic!("Error reading dir '{}': '{}'", current_folder.display(), e)
                            }
                        };
                        let mut files = vec![];
                        for file in files_raw {
                            let mut settings = HashMap::new();
                            let path = file.unwrap().path();
                            if !path.is_file() || path.extension().unwrap() != "md" {
                                continue;
                            }
                            let content = fs::read_to_string(&path).unwrap();
                            for line in content.split('\n') {
                                if line.contains('=')
                                    && !line.starts_with(' ')
                                    && !line.starts_with('>')
                                {
                                    let spls: Vec<&str> = line.split('=').collect();
                                    if spls.len() != 2 {
                                        continue;
                                    }
                                    settings.insert(spls[0].to_string(), spls[1].to_string());
                                }
                            }
                            settings.insert(
                                "url".to_string(),
                                path.strip_prefix("include")
                                    .unwrap()
                                    .with_extension("html")
                                    .display()
                                    .to_string(),
                            );
                            files.push(settings);
                        }
                        let smrli = rli + 1;
                        for file in files {
                            rli = smrli;
                            while raw_lines[rli] != ";;;" {
                                let mut line = raw_lines[rli].to_string();
                                for spl in line.clone().split("{{") {
                                    if spl.contains("}}") {
                                        let var = spl.split("}}").next().unwrap();
                                        if let Some(value) = file.get(var) {
                                            line = line
                                                .replace(
                                                    &format!("{{{{{}}}}}", var.to_string())
                                                        .to_string(),
                                                    value,
                                                )
                                                .to_string();
                                        }
                                    }
                                }
                                lines.push(line);
                                rli += 1;
                            }
                        }
                    }
                    if line.contains("{{") {
                        for spl in line.clone().split("{{") {
                            if spl.contains("}}") {
                                let var = spl.split("}}").next().unwrap();
                                if let Some(value) = folder.get(var) {
                                    line = line
                                        .replace(
                                            &format!("{{{{{}}}}}", var.to_string()).to_string(),
                                            value,
                                        )
                                        .to_string();
                                }
                            }
                        }
                    }
                    if print_line {
                        lines.push(line.to_string());
                    }
                    rli += 1;
                }
                rli += 1;
            }
        } else {
            lines.push(line.to_string());
        }
        rli += 1;
    }

    let mut embeded_lines = vec![];
    for line_raw in &lines {
        if line_raw.len() == 0 {
            continue;
        }
        let mut l = line_raw.len();
        let line = line_raw.trim_start();
        l -= line.len();

        let tag = line.contains(">");

        //variables
        if line.contains('=') && l == 0 && !tag {
            let v: Vec<&str> = line.split('=').collect();
            if v.len() > 1 {
                variables.insert(v[0].to_string(), v[1].to_string());
            }
        } else
        //embed
        if line.contains("++") && !tag {
            let name = line.replace("++", "");
            if let Some(emb_lines) = embed.get(&name) {
                //replace embeded line variables
                for emb_line in emb_lines {
                    let replaced = replace_variables(&variables, emb_line.clone());
                    embeded_lines.push(repeat_spaces(replaced, l));
                }
            }
        } else {
            let replaced = replace_variables(&variables, line.to_string());
            embeded_lines.push(repeat_spaces(replaced, l));
        }
    }

    let e = parse(embeded_lines);

    let path = path.strip_prefix("include").unwrap();
    let path = path::Path::new("public/").join(path.with_extension("html"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .append(false)
        .truncate(true)
        .create(true)
        .open(path)
        .unwrap();
    write!(file, "<!DOCTYPE html>\n<html>\n").unwrap();
    for e in e {
        write!(file, "{}\n", e).unwrap();
    }
    write!(file, "</html>").unwrap();
}

fn repeat_spaces(v: String, l: usize) -> String {
    let mut res = " ".repeat(l);
    res.push_str(&v);
    res
}

fn replace_variables(variables: &HashMap<String, String>, line: String) -> String {
    if line.contains("{{") {
        let mut emb_line_rep = line.clone();
        let emb_var_lines = line.split("{{");
        for emb_var_line in emb_var_lines {
            if emb_var_line.contains("}}") {
                let mut emb_var_line_spls = emb_var_line.split("}}");
                if let Some(key) = emb_var_line_spls.next() {
                    if let Some(value) = variables.get(key) {
                        emb_line_rep = emb_line_rep.replace(&format!("{{{{{}}}}}", key), value);
                    }
                }
            }
        }
        emb_line_rep
    } else {
        line
    }
}

fn get_element_in_level<'a>(er: &'a mut Vec<Element>, lr: usize) -> &'a mut Element {
    let erl = er.len();
    let mut r: &mut Element = &mut er[if erl == 0 { 0 } else { erl - 1 }];
    let mut l = 1;
    while l < lr {
        let s = r.children.len();
        r = &mut r.children[s - 1];
        l += 1;
    }
    r
}
