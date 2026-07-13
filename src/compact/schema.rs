use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompactRepoIndex {
    pub repo: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub generated_at: String,
    pub pk: Vec<CompactPackageSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompactPackageSummary {
    pub n: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub p: String,
    pub f: usize,
    pub s: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompactPackageDetail {
    pub n: String,
    pub f: Vec<CompactFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompactFileEntry {
    pub p: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sy: Vec<CompactSymbolEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub d: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub b: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompactSymbolEntry {
    pub n: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub g: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<String>,
    pub l: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub c: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompactQueryMatch {
    pub k: char,
    pub n: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub d: String,
    pub s: f32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub h: String,
}
