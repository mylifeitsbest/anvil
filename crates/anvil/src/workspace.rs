//! Открытая папка: дерево файлов (лениво), индекс для быстрого открытия, ветка git.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub depth: usize,
    pub entry: Entry,
    pub open: bool,
    /// Когда раскрыли родителя — для анимации появления.
    pub parent_opened: f64,
    pub index_in_parent: usize,
}

pub struct Workspace {
    pub root: Option<PathBuf>,
    pub name: String,
    pub expanded: HashSet<PathBuf>,
    pub opened_at: HashMap<PathBuf, f64>,
    cache: HashMap<PathBuf, Vec<Entry>>,
    pub exclude: Vec<String>,
    pub show_hidden: bool,
    pub index: Arc<Mutex<Vec<PathBuf>>>,
    pub indexing: Arc<AtomicBool>,
    pub branch: Option<String>,
}

impl Workspace {
    pub fn empty() -> Self {
        Self {
            root: None,
            name: String::new(),
            expanded: HashSet::new(),
            opened_at: HashMap::new(),
            cache: HashMap::new(),
            exclude: Vec::new(),
            show_hidden: false,
            index: Arc::new(Mutex::new(Vec::new())),
            indexing: Arc::new(AtomicBool::new(false)),
            branch: None,
        }
    }

    pub fn open(root: &Path, exclude: Vec<String>, show_hidden: bool, now: f64, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let root = root.canonicalize().map(strip_unc).unwrap_or_else(|_| root.to_path_buf());
        let mut ws = Self::empty();
        ws.name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root.to_string_lossy().to_string());
        ws.exclude = exclude;
        ws.show_hidden = show_hidden;
        ws.expanded.insert(root.clone());
        ws.opened_at.insert(root.clone(), now);
        ws.branch = read_branch(&root);
        ws.root = Some(root);
        ws.reindex(wake);
        ws
    }

    /// Перестроить индекс файлов в фоне (учитывает .gitignore).
    pub fn reindex(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        let Some(root) = self.root.clone() else { return };
        let index = self.index.clone();
        let flag = self.indexing.clone();
        let exclude = self.exclude.clone();
        let hidden = !self.show_hidden;
        flag.store(true, Ordering::Relaxed);
        std::thread::spawn(move || {
            let mut files = Vec::new();
            let walker = ignore::WalkBuilder::new(&root)
                .hidden(hidden)
                .git_ignore(true)
                .filter_entry(move |e| !exclude.iter().any(|x| e.file_name().to_string_lossy() == *x))
                .build();
            for e in walker.flatten() {
                if e.file_type().is_some_and(|t| t.is_file()) {
                    files.push(e.into_path());
                    if files.len() >= 200_000 {
                        break;
                    }
                }
            }
            files.sort();
            if let Ok(mut g) = index.lock() {
                *g = files;
            }
            flag.store(false, Ordering::Relaxed);
            wake();
        });
    }

    fn list(&self, dir: &Path) -> Vec<Entry> {
        let mut v: Vec<Entry> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        if self.exclude.contains(&name) || (!self.show_hidden && name.starts_with('.')) {
                            return None;
                        }
                        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        Some(Entry { path: e.path(), name, is_dir })
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        v
    }

    fn children(&mut self, dir: &Path) -> Vec<Entry> {
        if let Some(c) = self.cache.get(dir) {
            return c.clone();
        }
        let c = self.list(dir);
        self.cache.insert(dir.to_path_buf(), c.clone());
        c
    }

    /// Видимые строки дерева (обход в глубину по раскрытым папкам).
    pub fn rows(&mut self) -> Vec<Row> {
        let mut out = Vec::new();
        let Some(root) = self.root.clone() else { return out };
        let t = self.opened_at.get(&root).copied().unwrap_or(0.0);
        self.walk(&root, 0, t, &mut out);
        out
    }

    fn walk(&mut self, dir: &Path, depth: usize, opened: f64, out: &mut Vec<Row>) {
        for (i, e) in self.children(dir).into_iter().enumerate() {
            let open = e.is_dir && self.expanded.contains(&e.path);
            let path = e.path.clone();
            out.push(Row { depth, entry: e, open, parent_opened: opened, index_in_parent: i });
            if open && out.len() < 20_000 {
                let t = self.opened_at.get(&path).copied().unwrap_or(0.0);
                self.walk(&path, depth + 1, t, out);
            }
        }
    }

    pub fn toggle(&mut self, dir: &Path, now: f64) {
        if !self.expanded.remove(dir) {
            self.expanded.insert(dir.to_path_buf());
            self.opened_at.insert(dir.to_path_buf(), now);
        }
    }

    /// Раскрыть папки до файла.
    pub fn reveal(&mut self, file: &Path, now: f64) {
        let Some(root) = self.root.clone() else { return };
        let mut p = file.parent();
        while let Some(d) = p {
            if !d.starts_with(&root) {
                break;
            }
            if self.expanded.insert(d.to_path_buf()) {
                self.opened_at.insert(d.to_path_buf(), now);
            }
            p = d.parent();
        }
    }

    pub fn refresh(&mut self) {
        self.cache.clear();
        if let Some(r) = &self.root {
            self.branch = read_branch(r);
        }
    }

    pub fn files(&self) -> Vec<PathBuf> {
        self.index.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn relative(&self, p: &Path) -> String {
        match &self.root {
            Some(r) => p.strip_prefix(r).unwrap_or(p).to_string_lossy().replace('\\', "/"),
            None => p.to_string_lossy().to_string(),
        }
    }
}

/// Убираем `\\?\` у путей после canonicalize на Windows.
pub fn strip_unc(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => p,
    }
}

/// Текущая ветка из `.git/HEAD` без запуска git.
pub fn read_branch(root: &Path) -> Option<String> {
    let mut dir = Some(root);
    while let Some(d) = dir {
        let head = d.join(".git").join("HEAD");
        if let Ok(s) = std::fs::read_to_string(&head) {
            let s = s.trim();
            return Some(match s.strip_prefix("ref: refs/heads/") {
                Some(b) => b.to_string(),
                None => s.chars().take(7).collect(),
            });
        }
        dir = d.parent();
    }
    None
}
