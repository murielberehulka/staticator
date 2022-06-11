use std::{
    collections::HashMap,
    fmt, fs,
    io::Write,
    path::{self, PathBuf},
    sync::{atomic::AtomicU16, Arc},
};

pub const TAB_SIZE: usize = 4;

struct Element {
    pub name: Option<String>,
    pub level: usize,
    pub attrs: String,
    pub content: Option<String>,
    pub children: Vec<Element>,
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
                    } else {
                        write!(f, "{}<{}{}", tabs, name, self.attrs,).unwrap();
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
                    } else {
                        write!(f, "/>").unwrap();
                    }
                }
            }
            None => {
                write!(f, "{}{}<br>", tabs, self.content.as_ref().unwrap()).unwrap();
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
    for line in lines {
        println!("'{}'", line);
        if line.len() == 0 {
            continue;
        }
        let mut l = line.len();
        let line = line.trim_start();
        l -= line.len();
        if e.len() == 2 {
            l += 4;
        }

        let words: Vec<&str> = line.split(' ').collect();
        if words.len() < 1 {
            continue;
        }
        let name = if words[0].starts_with('>') {
            Some(words[0].trim_start_matches('>').to_string())
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
            while wi < ws {
                values.push(words[wi]);
                if let Some(_) = words[wi].find('"') {
                    break;
                }
                wi += 1;
            }
            let values = values.join(" ");
            match values.find("=\"") {
                Some(_) => attrs.push(values),
                None => contents.push(values),
            }
            wi += 1;
        }
        let element = Element {
            name,
            level: l / 4,
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
        };
        if l == 0 {
            e.push(element);
        } else {
            get_element_in_level(&mut e, l / TAB_SIZE)
                .children
                .push(element);
        }
    }
    e
}

pub fn file(path: PathBuf, embed: Arc<HashMap<String, Vec<String>>>) {
    println!("Compiling md file: {}", path.display());
    let mut variables = HashMap::<String, String>::new();
    let data = fs::read_to_string(&path).unwrap();
    let mut lines = Vec::new();
    for line_raw in data.split('\n') {
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
                    lines.push(repeat_spaces(replaced, l));
                }
            }
        } else {
            let replaced = replace_variables(&variables, line.to_string());
            lines.push(repeat_spaces(replaced, l));
        }
    }

    let e = parse(lines);

    let path = path.strip_prefix("include").unwrap();
    let path = path::Path::new("public/").join(path.with_extension("html"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .append(false)
        .truncate(true)
        .create(true)
        .open(path)
        .unwrap();
    write!(file, "<!DOCTYPE html><html>\n").unwrap();
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
