use serde::Deserialize;
use std::fmt;

/// Selected infrastructure fields of a droplet; unmodeled provider fields are ignored.
#[derive(Debug, Deserialize)]
pub struct Droplet {
    /// Provider identifier, used by the next workflow step.
    pub id: u64,
    /// Human-readable name.
    pub name: String,
    /// Open-ended provider status string.
    pub status: String,
    /// Memory in MiB.
    pub memory: u64,
    /// Virtual CPUs.
    pub vcpus: u32,
    /// Disk capacity in GiB.
    pub disk: u64,
    /// Provider timestamp, retained without lossy conversion.
    pub created_at: String,
    /// Size identifier.
    pub size_slug: String,
    /// Deployment region.
    pub region: Region,
    /// Network interfaces.
    pub networks: Networks,
    /// Droplet tags.
    pub tags: Vec<String>,
}

/// Selected region identity fields.
#[derive(Debug, Deserialize)]
pub struct Region {
    /// Machine-readable identifier.
    pub slug: String,
    /// Display name.
    pub name: String,
}

/// Droplet network families.
#[derive(Debug, Deserialize)]
pub struct Networks {
    /// IPv4 interfaces (the provider may omit this family).
    #[serde(default)]
    pub v4: Vec<Ipv4Network>,
    /// IPv6 interfaces (the provider may omit this family).
    #[serde(default)]
    pub v6: Vec<Ipv6Network>,
}

/// An IPv4 interface; the provider marks interface fields optional.
#[derive(Debug, Deserialize)]
pub struct Ipv4Network {
    /// Assigned address.
    pub ip_address: Option<String>,
    /// Netmask.
    pub netmask: Option<String>,
    /// Private interfaces may have no gateway.
    pub gateway: Option<String>,
    /// Public or private, retained as an extensible string.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// An IPv6 interface.
#[derive(Debug, Deserialize)]
pub struct Ipv6Network {
    /// Assigned address.
    pub ip_address: Option<String>,
    /// Prefix length.
    pub netmask: Option<u8>,
    /// Gateway.
    pub gateway: Option<String>,
    /// Interface type.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// An action observed on a droplet. Status/type strings remain open to future values.
#[derive(Debug, Deserialize)]
pub struct Action {
    /// Action identifier for the retrieval step.
    pub id: u64,
    /// Provider execution status.
    pub status: String,
    /// Action type, such as `reboot`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Start timestamp as supplied.
    pub started_at: String,
    /// Null while unfinished.
    pub completed_at: Option<String>,
    /// A deleted resource may have no identifier.
    pub resource_id: Option<u64>,
    /// Resource kind.
    pub resource_type: Option<String>,
    /// Region when available.
    pub region: Option<Region>,
    /// Region identifier when available.
    pub region_slug: Option<String>,
}

/// Single-droplet response envelope.
#[derive(Debug, Deserialize)]
pub struct DropletDocument {
    /// Requested droplet.
    pub droplet: Droplet,
}

/// Single-action response envelope.
#[derive(Debug, Deserialize)]
pub struct ActionDocument {
    /// Requested action.
    pub action: Action,
}

/// Collection totals, independent of continuation.
#[derive(Debug, Deserialize)]
pub struct CollectionMeta {
    /// Total matching records, not a page count.
    pub total: u64,
}

/// Body links. Debug withholds the server-supplied URLs.
#[derive(Default, Deserialize)]
pub struct Links {
    /// Missing pages means there is no continuation.
    pub pages: Option<PageLinks>,
}
impl fmt::Debug for Links {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Links(<redacted>)")
    }
}

/// Only `next` controls traversal; other relations are ignored.
#[derive(Default, Deserialize)]
pub struct PageLinks {
    /// Absent or null means completion. A present empty/invalid URL is an error.
    pub next: Option<String>,
}
impl fmt::Debug for PageLinks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PageLinks(<redacted>)")
    }
}

/// A page of droplets.
#[derive(Debug, Deserialize)]
pub struct Droplets {
    /// Records, including a possible empty intermediate page.
    pub droplets: Vec<Droplet>,
    /// Omitted when there is no pagination.
    pub links: Option<Links>,
    /// Optional provider collection totals.
    pub meta: Option<CollectionMeta>,
}

/// A page of actions for the selected droplet.
#[derive(Debug, Deserialize)]
pub struct Actions {
    /// Action records.
    pub actions: Vec<Action>,
    /// Omitted when there is no pagination.
    pub links: Option<Links>,
    /// Optional provider collection totals.
    pub meta: Option<CollectionMeta>,
}
