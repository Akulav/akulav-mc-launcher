//! Minecraft version JSON model, rule evaluation and `inheritsFrom` merging.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jar: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub main_class: Option<String>,
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub downloads: Option<VersionDownloads>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: ArgValue },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionDownloads {
    #[serde(default)]
    pub client: Option<DownloadInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DownloadInfo {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<DownloadInfo>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, DownloadInfo>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    /// Maven repository base URL (Fabric/Quilt style libraries).
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    #[serde(default = "default_component")]
    pub component: String,
    #[serde(default)]
    pub major_version: u32,
}

fn default_component() -> String {
    "jre-legacy".into()
}

// ---------------------------------------------------------------------------------------------
// Environment & rules
// ---------------------------------------------------------------------------------------------

/// The platform/feature set rules are evaluated against.
#[derive(Debug, Clone)]
pub struct Env {
    /// "windows", "osx" or "linux"
    pub os: &'static str,
    /// "x86", "x86_64", "arm64"
    pub arch: &'static str,
    pub os_version: String,
    pub features: HashMap<String, bool>,
}

impl Env {
    pub fn current() -> Self {
        let os = match std::env::consts::OS {
            "windows" => "windows",
            "macos" => "osx",
            _ => "linux",
        };
        let arch = match std::env::consts::ARCH {
            "x86" => "x86",
            "aarch64" => "arm64",
            _ => "x86_64",
        };
        Self { os, arch, os_version: String::new(), features: HashMap::new() }
    }

    pub fn with_feature(mut self, name: &str, on: bool) -> Self {
        self.features.insert(name.to_string(), on);
        self
    }

    /// The `${arch}` substitution used in legacy native classifiers.
    pub fn arch_bits(&self) -> &'static str {
        if self.arch == "x86" {
            "32"
        } else {
            "64"
        }
    }
}

impl Rule {
    fn matches(&self, env: &Env) -> bool {
        if let Some(os) = &self.os {
            if let Some(name) = &os.name {
                if name != env.os {
                    return false;
                }
            }
            if let Some(arch) = &os.arch {
                if arch != env.arch {
                    return false;
                }
            }
            if let (Some(pattern), false) = (&os.version, env.os_version.is_empty()) {
                if let Ok(re) = regex::Regex::new(pattern) {
                    if !re.is_match(&env.os_version) {
                        return false;
                    }
                }
            }
        }
        if let Some(features) = &self.features {
            for (k, v) in features {
                if env.features.get(k).copied().unwrap_or(false) != *v {
                    return false;
                }
            }
        }
        true
    }
}

/// Mojang rule semantics: no rules → allowed; otherwise the last matching rule decides,
/// and if none match the result is "disallowed".
pub fn rules_allow(rules: &[Rule], env: &Env) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule.matches(env) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

impl Argument {
    pub fn values(&self, env: &Env) -> Vec<String> {
        match self {
            Argument::Plain(s) => vec![s.clone()],
            Argument::Conditional { rules, value } => {
                if !rules_allow(rules, env) {
                    return vec![];
                }
                match value {
                    ArgValue::One(s) => vec![s.clone()],
                    ArgValue::Many(v) => v.clone(),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Maven coordinates
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
}

impl MavenCoord {
    pub fn parse(name: &str) -> Option<Self> {
        let (coords, extension) = match name.rsplit_once('@') {
            Some((c, e)) => (c, e.to_string()),
            None => (name, "jar".to_string()),
        };
        let parts: Vec<&str> = coords.split(':').collect();
        if parts.len() < 3 {
            return None;
        }
        Some(Self {
            group: parts[0].to_string(),
            artifact: parts[1].to_string(),
            version: parts[2].to_string(),
            classifier: parts.get(3).map(|s| s.to_string()),
            extension,
        })
    }

    /// Relative path inside a maven repository / `libraries` folder (always `/`-separated).
    pub fn path(&self) -> String {
        let mut file = format!("{}-{}", self.artifact, self.version);
        if let Some(c) = &self.classifier {
            file.push('-');
            file.push_str(c);
        }
        file.push('.');
        file.push_str(&self.extension);
        format!("{}/{}/{}/{}", self.group.replace('.', "/"), self.artifact, self.version, file)
    }

    pub fn with_classifier(&self, classifier: &str) -> Self {
        Self { classifier: Some(classifier.to_string()), ..self.clone() }
    }

    /// Identity used to de-duplicate libraries across versions (ignores the version number).
    pub fn dedupe_key(&self) -> String {
        format!(
            "{}:{}:{}@{}",
            self.group,
            self.artifact,
            self.classifier.as_deref().unwrap_or(""),
            self.extension
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Library → files
// ---------------------------------------------------------------------------------------------

pub const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryFile {
    /// Relative to the libraries folder, `/`-separated.
    pub path: String,
    /// `None` when the file has no download (e.g. produced by a Forge installer).
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub classpath: bool,
    /// Extract this archive's native libraries into the natives folder.
    pub natives: bool,
    pub exclude: Vec<String>,
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().filter(|s| !s.trim().is_empty()).cloned()
}

fn join_url(base: &str, path: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

impl Library {
    pub fn coord(&self) -> Option<MavenCoord> {
        MavenCoord::parse(&self.name)
    }

    pub fn applies(&self, env: &Env) -> bool {
        self.rules.as_deref().is_none_or(|r| rules_allow(r, env))
    }

    /// The files this library contributes on the current platform.
    pub fn files(&self, env: &Env) -> Vec<LibraryFile> {
        let mut out = Vec::new();
        if !self.applies(env) {
            return out;
        }
        let Some(coord) = self.coord() else { return out };
        let exclude = self.extract.as_ref().map(|e| e.exclude.clone()).unwrap_or_default();
        let modern_native = coord.classifier.as_deref().is_some_and(|c| c.starts_with("natives-"));
        let downloads = self.downloads.as_ref();

        // Main artifact.
        let has_legacy_natives = self.natives.is_some();
        if let Some(artifact) = downloads.and_then(|d| d.artifact.as_ref()) {
            out.push(LibraryFile {
                path: non_empty(&artifact.path).unwrap_or_else(|| coord.path()),
                url: non_empty(&artifact.url),
                sha1: non_empty(&artifact.sha1),
                size: artifact.size,
                classpath: true,
                natives: modern_native,
                exclude: exclude.clone(),
            });
        } else if !has_legacy_natives && downloads.is_none_or(|d| d.classifiers.is_none()) {
            // Plain maven library (Fabric, Quilt, old Forge): resolve from its repository.
            let path = coord.path();
            let base = non_empty(&self.url).unwrap_or_else(|| MOJANG_LIBRARIES.to_string());
            out.push(LibraryFile {
                url: Some(join_url(&base, &path)),
                path,
                sha1: non_empty(&self.sha1),
                size: self.size,
                classpath: true,
                natives: modern_native,
                exclude: exclude.clone(),
            });
        }

        // Legacy natives (pre-1.19): classifier chosen per OS.
        if let Some(classifier) = self.natives.as_ref().and_then(|n| n.get(env.os)) {
            let classifier = classifier.replace("${arch}", env.arch_bits());
            let info = downloads.and_then(|d| d.classifiers.as_ref()).and_then(|c| c.get(&classifier));
            let ncoord = coord.with_classifier(&classifier);
            let file = match info {
                Some(i) => LibraryFile {
                    path: non_empty(&i.path).unwrap_or_else(|| ncoord.path()),
                    url: non_empty(&i.url),
                    sha1: non_empty(&i.sha1),
                    size: i.size,
                    classpath: false,
                    natives: true,
                    exclude,
                },
                None => {
                    let path = ncoord.path();
                    let base = non_empty(&self.url).unwrap_or_else(|| MOJANG_LIBRARIES.to_string());
                    LibraryFile { url: Some(join_url(&base, &path)), path, sha1: None, size: None, classpath: false, natives: true, exclude }
                }
            };
            out.push(file);
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Inheritance
// ---------------------------------------------------------------------------------------------

/// A version with its parents merged in.
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub json: VersionJson,
    /// Version chain from the requested id down to the base game version.
    #[allow(dead_code)]
    pub chain: Vec<String>,
    /// Id of the vanilla version that provides the client jar.
    pub jar_source: String,
}

/// Merge `child` over `parent` the way the official launcher does.
pub fn merge(child: VersionJson, parent: VersionJson) -> VersionJson {
    let mut seen: HashSet<String> = HashSet::new();
    let mut libraries = Vec::with_capacity(child.libraries.len() + parent.libraries.len());
    for lib in child.libraries.into_iter().chain(parent.libraries) {
        let key = lib.coord().map(|c| c.dedupe_key()).unwrap_or_else(|| lib.name.clone());
        // Natives-only libraries with rules share a coordinate with their non-native twin in old
        // versions; keep them both by including the rules in the key.
        let key = if lib.natives.is_some() { format!("{key}#natives") } else { key };
        if seen.insert(key) {
            libraries.push(lib);
        }
    }

    let arguments = match (child.arguments, parent.arguments) {
        (Some(c), Some(p)) => Some(Arguments {
            game: p.game.into_iter().chain(c.game).collect(),
            jvm: p.jvm.into_iter().chain(c.jvm).collect(),
        }),
        (c, p) => c.or(p),
    };

    VersionJson {
        id: child.id,
        inherits_from: parent.inherits_from,
        jar: child.jar.or(parent.jar),
        kind: child.kind.or(parent.kind),
        main_class: child.main_class.or(parent.main_class),
        minecraft_arguments: child.minecraft_arguments.or(parent.minecraft_arguments),
        arguments,
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        downloads: child.downloads.or(parent.downloads),
        libraries,
        java_version: child.java_version.or(parent.java_version),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win() -> Env {
        Env { os: "windows", arch: "x86_64", os_version: String::new(), features: HashMap::new() }
    }
    fn linux() -> Env {
        Env { os: "linux", arch: "x86_64", os_version: String::new(), features: HashMap::new() }
    }

    #[test]
    fn maven_paths() {
        let c = MavenCoord::parse("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap();
        assert_eq!(c.path(), "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar");
        let c = MavenCoord::parse("net.neoforged:neoform:1.21.1-20240808.144430@zip").unwrap();
        assert_eq!(c.path(), "net/neoforged/neoform/1.21.1-20240808.144430/neoform-1.21.1-20240808.144430.zip");
        assert!(MavenCoord::parse("broken:name").is_none());
    }

    #[test]
    fn rule_semantics() {
        let rules: Vec<Rule> = serde_json::from_str(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#).unwrap();
        assert!(rules_allow(&rules, &win()));
        let mac = Env { os: "osx", ..win() };
        assert!(!rules_allow(&rules, &mac));

        let only_linux: Vec<Rule> = serde_json::from_str(r#"[{"action":"allow","os":{"name":"linux"}}]"#).unwrap();
        assert!(!rules_allow(&only_linux, &win()));
        assert!(rules_allow(&only_linux, &linux()));

        let feat: Vec<Rule> = serde_json::from_str(r#"[{"action":"allow","features":{"has_custom_resolution":true}}]"#).unwrap();
        assert!(!rules_allow(&feat, &win()));
        assert!(rules_allow(&feat, &win().with_feature("has_custom_resolution", true)));

        let x86: Vec<Rule> = serde_json::from_str(r#"[{"action":"allow","os":{"arch":"x86"}}]"#).unwrap();
        assert!(!rules_allow(&x86, &win()));
    }

    #[test]
    fn modern_arguments_parse() {
        let json = r#"{"game":["--username","${auth_player_name}",{"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"},
            {"rules":[{"action":"allow","features":{"has_custom_resolution":true}}],"value":["--width","${resolution_width}"]}],
            "jvm":[{"rules":[{"action":"allow","os":{"name":"windows"}}],"value":"-XX:HeapDumpPath=x"},"-cp","${classpath}"]}"#;
        let a: Arguments = serde_json::from_str(json).unwrap();
        let env = win();
        let game: Vec<String> = a.game.iter().flat_map(|x| x.values(&env)).collect();
        assert_eq!(game, vec!["--username", "${auth_player_name}"]);
        let env2 = win().with_feature("has_custom_resolution", true);
        let game: Vec<String> = a.game.iter().flat_map(|x| x.values(&env2)).collect();
        assert_eq!(game.len(), 4);
        let jvm: Vec<String> = a.jvm.iter().flat_map(|x| x.values(&linux())).collect();
        assert_eq!(jvm, vec!["-cp", "${classpath}"]);
    }

    #[test]
    fn library_files_modern_and_legacy() {
        // Modern vanilla library with artifact + os rule
        let lib: Library = serde_json::from_str(r#"{"name":"org.lwjgl:lwjgl:3.3.3:natives-windows",
            "downloads":{"artifact":{"path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar","sha1":"abc","size":10,"url":"https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"}},
            "rules":[{"action":"allow","os":{"name":"windows"}}]}"#).unwrap();
        let f = lib.files(&win());
        assert_eq!(f.len(), 1);
        assert!(f[0].classpath && f[0].natives);
        assert!(lib.files(&linux()).is_empty());

        // Legacy natives
        let lib: Library = serde_json::from_str(r#"{"name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
            "natives":{"linux":"natives-linux","windows":"natives-windows-${arch}"},
            "downloads":{"classifiers":{"natives-windows-64":{"path":"p/w64.jar","url":"https://x/w64.jar","sha1":"s","size":1}}}}"#).unwrap();
        let f = lib.files(&win());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].path, "p/w64.jar");
        assert!(!f[0].classpath && f[0].natives);

        // Fabric-style maven library
        let lib: Library = serde_json::from_str(r#"{"name":"net.fabricmc:fabric-loader:0.16.9","url":"https://maven.fabricmc.net/"}"#).unwrap();
        let f = lib.files(&win());
        assert_eq!(f[0].url.as_deref(), Some("https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"));

        // Forge-generated library: empty URL means "must exist locally"
        let lib: Library = serde_json::from_str(r#"{"name":"net.neoforged:neoforge:21.1.248:client",
            "downloads":{"artifact":{"path":"net/neoforged/neoforge/21.1.248/neoforge-21.1.248-client.jar","url":"","sha1":"x","size":1}}}"#).unwrap();
        let f = lib.files(&win());
        assert_eq!(f[0].url, None);
    }

    #[test]
    fn merge_child_over_parent() {
        let parent: VersionJson = serde_json::from_str(r#"{"id":"1.21.1","type":"release","mainClass":"net.minecraft.client.main.Main",
            "assetIndex":{"id":"17","url":"u"},"assets":"17","downloads":{"client":{"url":"c","sha1":"s","size":1}},
            "javaVersion":{"component":"java-runtime-delta","majorVersion":21},
            "arguments":{"game":["--username","${auth_player_name}"],"jvm":["-cp","${classpath}"]},
            "libraries":[{"name":"org.ow2.asm:asm:9.3"},{"name":"com.mojang:brigadier:1.3.10"}]}"#).unwrap();
        let child: VersionJson = serde_json::from_str(r#"{"id":"neoforge-21.1.248","inheritsFrom":"1.21.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
            "arguments":{"game":["--fml.neoForgeVersion","21.1.248"],"jvm":["-DignoreList=x"]},
            "libraries":[{"name":"org.ow2.asm:asm:9.7"}]}"#).unwrap();
        let m = merge(child, parent);
        assert_eq!(m.id, "neoforge-21.1.248");
        assert_eq!(m.main_class.as_deref(), Some("cpw.mods.bootstraplauncher.BootstrapLauncher"));
        assert_eq!(m.libraries.len(), 2);
        assert_eq!(m.libraries[0].name, "org.ow2.asm:asm:9.7");
        let a = m.arguments.unwrap();
        assert_eq!(a.game.len(), 4);
        assert_eq!(a.jvm.len(), 3);
        assert_eq!(m.java_version.unwrap().major_version, 21);
        assert!(m.inherits_from.is_none());
    }
}
