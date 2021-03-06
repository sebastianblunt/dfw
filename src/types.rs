// Copyright 2017 - 2019 Pit Kleyersburg <pitkley@googlemail.com>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified or distributed
// except according to those terms.

//! The types in this module make up the structure of the configuration-file(s).
//!
//! # Example
//!
//! The following is an examplary TOML configuration, which will be parsed into this modules types.
//!
//! ```toml
//! [defaults]
//! custom_tables = { name = "filter", chains = ["input", "forward"]}
//! external_network_interfaces = "eth0"
//!
//! [initialization]
//! rules = [
//!     "add table inet custom",
//! ]
//!
//! [container_to_container]
//! default_policy = "drop"
//!
//! [[container_to_container.rules]]
//! network = "common_network"
//! src_container = "container_a"
//! dst_container = "container_b"
//! verdict = "accept"
//!
//! [container_to_wider_world]
//! default_policy = "accept"
//!
//! [[container_to_container.rules]]
//! network = "other_network"
//! src_container = "container_c"
//! verdict = "drop"
//!
//! [wider_world_to_container]
//!
//! [[wider_world_to_container.rules]]
//! network = "common_network"
//! dst_container = "container_a"
//! expose_port = [80, 443]
//!
//! [container_dnat]
//!
//! [[container_dnat.rules]]
//! src_network = "common_network"
//! src_container = "container_a"
//! dst_network = "other_network"
//! dst_container = "container_c"
//! expose_port = { host_port = 8080, container_port = 80, family = "tcp" }
//! ```

use crate::nftables::*;
use derive_builder::Builder;
use serde::{de, Deserialize};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

const DEFAULT_PROTOCOL: &str = "tcp";

/// `DFW` is the parent type defining the complete configuration used by DFW to build up the
/// firewall rules.
///
/// Every section is optional.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DFW {
    /// The `defaults` configuration section
    #[serde(default)]
    pub defaults: Option<Defaults>,
    /// The `initialization` configuration section
    #[serde(default)]
    pub initialization: Option<Initialization>,
    /// The `container_to_container` configuration section
    pub container_to_container: Option<ContainerToContainer>,
    /// The `container_to_wider_world` configuration section
    pub container_to_wider_world: Option<ContainerToWiderWorld>,
    /// The `container_to_host` configuration section
    pub container_to_host: Option<ContainerToHost>,
    /// The `wider_world_to_container` configuration section
    pub wider_world_to_container: Option<WiderWorldToContainer>,
    /// The `container_dnat` configuration section
    pub container_dnat: Option<ContainerDNAT>,
}

/// The default configuration section, used by DFW for rule processing.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// Specify the names of custom nft-tables that should be partially managed.
    ///
    /// # Explanation
    ///
    /// If you want to use or already use an existing nftables table to manage rules independently
    /// from DFW, it is important that two conditions are met:
    ///
    /// 1. The priority-values of the chains are _lower_ than the priority-values used by DFW.
    /// 2. The default-policy of the any input or forward chains in the table are set to `accept`.
    ///
    /// While DFW cannot ensure that the first condition is met (since changing the priority of a
    /// chain is not possible without recreating the chain), it can set the policies of your input
    /// and output chains to `accept` for you.
    ///
    /// # Example
    ///
    /// ```toml
    /// custom_tables = { name = "filter", chains = ["input", "forward"] }
    /// custom_tables = [
    ///     { name = "filter", chains = ["input", "forward"] },
    ///     { name = "custom", chains = ["input", "forward"] }
    /// ]
    /// ```
    #[serde(default, deserialize_with = "option_struct_or_seq_struct")]
    pub custom_tables: Option<Vec<Table>>,

    /// This defines the external network interfaces of the host to consider during building the
    /// rules. The value can be non-existant, a string, or a sequence of strings.
    ///
    /// # Example
    ///
    /// ```toml
    /// external_network_interfaces = "eth0"
    /// external_network_interfaces = ["eth0", "eth1"]
    /// ```
    #[serde(default, deserialize_with = "option_string_or_seq_string")]
    pub external_network_interfaces: Option<Vec<String>>,

    /// This defines whether the default Docker bridge (usually `docker0`) is allowed to access host
    /// resources.
    ///
    /// For non-default Docker bridges this is controlled within the [container-to-host section].
    ///
    /// [container-to-host section]: struct.ContainerToHostRule.html
    #[serde(default)]
    pub default_docker_bridge_to_host_policy: ChainPolicy,
}

/// Reference to an nftables table, specifically to the input- and forward-chains within it.
///
/// This is used by DFW when managing other tables is required.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// Name of the custom table.
    pub name: String,

    /// Names of the input and forward chains defined within the custom table.
    pub chains: Vec<String>,
}

/// The initialization section allows you to execute any commands against nftables.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct Initialization {
    /// Initialization rules for nftables
    ///
    /// # Example
    ///
    /// ```toml
    /// [initialization]
    /// rules = [
    ///     "add table inet custom",
    ///     "flush table inet custom",
    ///     # ...
    /// ]
    /// ```
    pub rules: Option<Vec<String>>,
}

/// The container-to-container section, defining how containers can communicate amongst each other.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ContainerToContainer {
    /// The `default_policy` defines the default for when there is not a specific rule.
    ///
    /// # Filtering traffic within the same bridge
    ///
    /// Depending on how your host is configured, traffic whose origin and destination interface are
    /// the same bridge is _not_ filtered by the kernel netfilter module. This means that this
    /// default policy is not honored for traffic between containers that are on the same Docker
    /// network, but only for traffic that traverses two bridges.
    ///
    /// If your kernel has the `br_netfilter` kernel-module available, you can set the sysctl
    /// `net.bridge.bridge-nf-call-iptables` to `1` to have the netfilter-module act on traffic
    /// within the same bridge, too. You can set this value temporarily like this:
    ///
    /// ```norun
    /// sysctl net.bridge.bridge-nf-call-iptables=1
    /// ```
    ///
    /// To permanently set this configuration, take a look at `man sysctl.d` and `man sysctl.conf`.
    pub default_policy: ChainPolicy,
    /// An optional list of rules, see
    /// [`ContainerToContainerRule`](struct.ContainerToContainerRule.html).
    ///
    /// # Example
    ///
    /// The easiest way to define the rules is using TOMLs [arrays of tables][toml-aot]:
    ///
    /// ```toml
    /// [[container_to_container.rules]]
    /// # first rule here
    /// [[container_to_container.rules]]
    /// # second rule here
    /// ```
    ///
    /// [toml-aot]:
    ///  https://github.com/toml-lang/toml/blob/master/versions/en/toml-v0.4.0.md#array-of-tables
    pub rules: Option<Vec<ContainerToContainerRule>>,
}

/// Definition for a rule to be used in the container-to-container section.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ContainerToContainerRule {
    /// Common network between the source container and the destination container to apply the rule
    /// to.
    pub network: String,
    /// Source container to apply the rule to.
    pub src_container: Option<String>,
    /// Destination container to apply the rule to.
    pub dst_container: Option<String>,
    /// Additional match-string, which will be added to the nftables command.
    pub matches: Option<String>,
    /// Verdict for rule (accept, drop or reject).
    #[serde(alias = "action")]
    pub verdict: RuleVerdict,
}

/// The container-to-wider-world section, defining how containers can communicate with the wider
/// world.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ContainerToWiderWorld {
    /// The `default_policy` defines the default for when there is not a specific rule.
    pub default_policy: RuleVerdict,
    /// An optional list of rules, see
    /// [`ContainerToWiderWorldRule`](struct.ContainerToWiderWorldRule.html).
    ///
    /// # Example
    ///
    /// The easiest way to define the rules is using TOMLs [arrays of tables][toml-aot]:
    ///
    /// ```toml
    /// [[container_to_wider_world.rules]]
    /// # first rule here
    /// [[container_to_wider_world.rules]]
    /// # second rule here
    /// ```
    ///
    /// [toml-aot]:
    ///  https://github.com/toml-lang/toml/blob/master/versions/en/toml-v0.4.0.md#array-of-tables
    pub rules: Option<Vec<ContainerToWiderWorldRule>>,
}

/// Definition for a rule to be used in the container-to-wider-world section.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ContainerToWiderWorldRule {
    /// Network of the source container to apply the rule to.
    pub network: Option<String>,
    /// Source container to apply the rule to.
    pub src_container: Option<String>,
    /// Additional match-string, which will be added to the nftables command.
    pub matches: Option<String>,
    /// Verdict for rule (accept, drop or reject).
    #[serde(alias = "action")]
    pub verdict: RuleVerdict,
    /// Specific external network interface to target.
    pub external_network_interface: Option<String>,
}

/// The container-to-host section, defining how containers can communicate with the host.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ContainerToHost {
    /// The `default_policy` defines the default for when there is not a specific rule.
    pub default_policy: RuleVerdict,
    /// An optional list of rules, see
    /// [`ContainerToHostRule`](struct.ContainerToHostRule.html).
    ///
    /// # Example
    ///
    /// The easiest way to define the rules is using TOMLs [arrays of tables][toml-aot]:
    ///
    /// ```toml
    /// [[container_to_host.rules]]
    /// # first rule here
    /// [[container_to_host.rules]]
    /// # second rule here
    /// ```
    ///
    /// [toml-aot]:
    ///  https://github.com/toml-lang/toml/blob/master/versions/en/toml-v0.4.0.md#array-of-tables
    pub rules: Option<Vec<ContainerToHostRule>>,
}

/// Definition for a rule to be used in the container-to-host section.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ContainerToHostRule {
    /// Network of the source container to apply the rule to.
    pub network: String,
    /// Source container to apply the rule to.
    pub src_container: Option<String>,
    /// Additional match-string, which will be added to the nftables command.
    pub matches: Option<String>,
    /// Verdict for rule (accept, drop or reject).
    #[serde(alias = "action")]
    pub verdict: RuleVerdict,
}

/// The wider-world-to-container section, defining how containers can reached from the wider world.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct WiderWorldToContainer {
    /// An optional list of rules, see
    /// [`WiderWorldToContainerRule`](struct.WiderWorldToContainerRule.html).
    ///
    /// # Example
    ///
    /// The easiest way to define the rules is using TOMLs [arrays of tables][toml-aot]:
    ///
    /// ```toml
    /// [[wider_world_to_container.rules]]
    /// # first rule here
    /// [[wider_world_to_container.rules]]
    /// # second rule here
    /// ```
    ///
    /// [toml-aot]:
    ///  https://github.com/toml-lang/toml/blob/master/versions/en/toml-v0.4.0.md#array-of-tables
    pub rules: Option<Vec<WiderWorldToContainerRule>>,
}

/// Definition for a rule to be used in the wider-world-to-container section.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct WiderWorldToContainerRule {
    /// Network of the destination container to apply the rule to.
    pub network: String,

    /// Destination container to apply the rule to.
    pub dst_container: String,

    /// Ports to apply the rule to.
    ///
    /// Defined as:
    ///
    /// * a single integer
    ///
    /// * a single string
    ///
    /// * a single struct
    ///
    /// * a list of integers
    ///
    /// * a list of strings
    ///
    /// * a list of structs
    ///
    /// # Example
    ///
    /// All of the following are legal TOML fragments:
    ///
    /// ```toml
    /// expose_port = 80
    /// expose_port = [80, 443]
    /// expose_port = "53/udp"
    /// expose_port = ["80/tcp", "53/udp"]
    ///
    /// # The following four all result in the same definition
    /// expose_port = { host_port = 8080 }
    /// expose_port = { host_port = 8080, container_port = 8080 }
    /// expose_port = { host_port = 8080, family = "tcp" }
    /// expose_port = { host_port = 8080, container_port = 8080, family = "tcp" }
    ///
    /// expose_port = [
    ///     { host_port = 80 },
    ///     { host_port = 53, family = "udp" },
    ///     { host_port = 443, container_port = 8443 },
    /// ]
    /// ```
    #[serde(deserialize_with = "single_or_seq_string_or_struct")]
    pub expose_port: Vec<ExposePort>,

    /// Specific external network interface to target.
    pub external_network_interface: Option<String>,

    /// Source CIDRs (IPv4) to which incoming traffic should be restricted.
    ///
    /// This can be:
    ///
    /// * a single string
    ///
    /// * a list of strings
    ///
    /// There is no validation whether the provided CIDRs are actually valid.
    ///
    /// # Example
    ///
    /// All of the following are legal TOML fragments:
    ///
    /// ```toml
    /// source_cidr_v4 = "127.0.0.0/8"
    ///
    /// source_cidr _v4= ["127.0.0.0/8", "192.0.2.1/32"]
    /// ```
    #[serde(
        default,
        deserialize_with = "option_string_or_seq_string",
        alias = "source_cidr"
    )]
    pub source_cidr_v4: Option<Vec<String>>,

    /// Source CIDRs (IPv6) to which incoming traffic should be restricted.
    ///
    /// This can be:
    ///
    /// * a single string
    ///
    /// * a list of strings
    ///
    /// There is no validation whether the provided CIDRs are actually valid.
    ///
    /// # Example
    ///
    /// All of the following are legal TOML fragments:
    ///
    /// ```toml
    /// source_cidr_v6 = "fe80::/10"
    ///
    /// source_cidr_v6 = ["fe80::/10", "2001:db8::/32"]
    /// ```
    #[serde(
        default,
        deserialize_with = "option_string_or_seq_string",
        alias = "source_cidr"
    )]
    pub source_cidr_v6: Option<Vec<String>>,
}

/// Struct to hold a port definition to expose on the host/between containers.
#[derive(Deserialize, Debug, Clone, Default, Builder, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ExposePort {
    /// Port the `container_port` should be exposed to on the host.
    #[builder(field(public))]
    pub host_port: u16,

    /// Port the `host_port` should map to into the container.
    #[builder(field(public), default = "self.default_container_port()?")]
    pub container_port: Option<u16>,

    /// Family of the exposed port.
    ///
    /// Can be left blank, `tcp` will be used as default.
    #[serde(default = "default_expose_port_family")]
    #[builder(field(public), default = "self.default_family()?")]
    pub family: String,
}

impl ExposePortBuilder {
    fn client_and_host_port(&mut self, value: &str) -> Result<&mut Self, String> {
        let split: Vec<&str> = value.split(':').collect();
        match split.len() {
            1 => self.host_port = Some(split[0].parse().map_err(|e| format!("{}", e))?),
            2 => {
                self.host_port = Some(split[0].parse().map_err(|e| format!("{}", e))?);
                self.container_port = Some(Some(split[1].parse().map_err(|e| format!("{}", e))?));
            }
            _ => return Err(format!("port string has invalid format '{}'", value)),
        }
        Ok(self)
    }

    fn default_container_port(&self) -> Result<Option<u16>, String> {
        Ok(None)
    }

    fn default_family(&self) -> Result<String, String> {
        Ok(DEFAULT_PROTOCOL.to_owned())
    }
}

impl FromStr for ExposePort {
    type Err = String;

    /// Convert a formatted string into a [`ExposePort`](struct.ExposePort.html).
    ///
    /// The string has to be in the format `<HOST_PORT>[:<CONTAINER_PORT>]/<FAMILY>`, i.e.
    /// `80:8080/tcp`. If you don't specify the container-port, it is assumed to be identical to the
    /// host-port.
    ///
    /// # Example
    ///
    /// ```
    /// # use dfw::types::ExposePort;
    /// let port: ExposePort = "80".parse().unwrap();
    /// assert_eq!(port.host_port, 80);
    /// assert_eq!(port.container_port, None);
    /// assert_eq!(port.family, "tcp");
    /// ```
    ///
    /// ```
    /// # use dfw::types::ExposePort;
    /// let port: ExposePort = "53/udp".parse().unwrap();
    /// assert_eq!(port.host_port, 53);
    /// assert_eq!(port.container_port, None);
    /// assert_eq!(port.family, "udp");
    /// ```
    ///
    /// ```
    /// # use dfw::types::ExposePort;
    /// let port: ExposePort = "80:8080/tcp".parse().unwrap();
    /// assert_eq!(port.host_port, 80);
    /// assert_eq!(port.container_port, Some(8080));
    /// assert_eq!(port.family, "tcp");
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split: Vec<&str> = s.split('/').collect();
        Ok(match split.len() {
            1 => ExposePortBuilder::default()
                .client_and_host_port(split[0])?
                .build()?,
            2 => ExposePortBuilder::default()
                .client_and_host_port(split[0])?
                .family(split[1].to_owned())
                .build()?,
            _ => return Err(format!("port string has invalid format '{}'", s)),
        })
    }
}

/// The container-DNAT section, defining how containers can communicate with each other over
/// non-common networks.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ContainerDNAT {
    /// An optional list of rules, see
    /// [`ContainerDNATRule`](struct.ContainerDNATRule.html).
    ///
    /// # Example
    ///
    /// The easiest way to define the rules is using TOMLs [arrays of tables][toml-aot]:
    ///
    /// ```toml
    /// [[container_dnat.rules]]
    /// # first rule here
    /// [[container_dnat.rules]]
    /// # second rule here
    /// ```
    ///
    /// [toml-aot]:
    ///  https://github.com/toml-lang/toml/blob/master/versions/en/toml-v0.4.0.md#array-of-tables
    pub rules: Option<Vec<ContainerDNATRule>>,
}

/// Definition for a rule to be used in the container-DNAT section.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ContainerDNATRule {
    /// Network of the source container to apply the rule to.
    pub src_network: Option<String>,

    /// Source container to apply the rule to.
    pub src_container: Option<String>,

    /// Network of the destination container to apply the rule to.
    pub dst_network: String,

    /// Destination container to apply the rule to.
    pub dst_container: String,

    /// Ports to apply the rule to.
    ///
    /// Defined as:
    ///
    /// * a single integer
    ///
    /// * a single string
    ///
    /// * a single struct
    ///
    /// * a list of integers
    ///
    /// * a list of strings
    ///
    /// * a list of structs
    ///
    /// # Example
    ///
    /// All of the following are legal TOML fragments:
    ///
    /// ```toml
    /// expose_port = 80
    /// expose_port = [80, 443]
    /// expose_port = "53/udp"
    /// expose_port = ["80/tcp", "53/udp"]
    ///
    /// # The following four all result in the same definition
    /// expose_port = { host_port = 8080 }
    /// expose_port = { host_port = 8080, container_port = 8080 }
    /// expose_port = { host_port = 8080, family = "tcp" }
    /// expose_port = { host_port = 8080, container_port = 8080, family = "tcp" }
    ///
    /// expose_port = [
    ///     { host_port = 80 },
    ///     { host_port = 53, family = "udp" },
    ///     { host_port = 443, container_port = 8443 },
    /// ]
    /// ```
    #[serde(deserialize_with = "single_or_seq_string_or_struct")]
    pub expose_port: Vec<ExposePort>,
}

fn default_expose_port_family() -> String {
    DEFAULT_PROTOCOL.to_owned()
}

struct StringOrStruct<T>(PhantomData<T>);

impl<'de, T> de::Visitor<'de> for StringOrStruct<T>
where
    T: de::Deserialize<'de> + FromStr<Err = String>,
{
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("integer, string or map")
    }

    fn visit_i64<E>(self, value: i64) -> Result<T, E>
    where
        E: de::Error,
    {
        FromStr::from_str(&value.to_string()).map_err(de::Error::custom)
    }

    fn visit_str<E>(self, value: &str) -> Result<T, E>
    where
        E: de::Error,
    {
        FromStr::from_str(value).map_err(de::Error::custom)
    }

    fn visit_map<M>(self, visitor: M) -> Result<T, M::Error>
    where
        M: de::MapAccess<'de>,
    {
        de::Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))
    }
}

// Thanks to @dtolnay for the support:
//   https://github.com/serde-rs/serde/issues/901#issuecomment-297070279
impl<'de, T> de::DeserializeSeed<'de> for StringOrStruct<T>
where
    T: de::Deserialize<'de> + FromStr<Err = String>,
{
    type Value = T;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

#[allow(dead_code)]
fn string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: de::Deserialize<'de> + FromStr<Err = String>,
    D: de::Deserializer<'de>,
{
    deserializer.deserialize_any(StringOrStruct(PhantomData))
}

struct SingleOrSeqStringOrStruct<T>(PhantomData<T>);

impl<'de, T> de::Visitor<'de> for SingleOrSeqStringOrStruct<T>
where
    T: de::Deserialize<'de> + FromStr<Err = String>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(
            "sequence of integers, strings or maps \
             or a single integer, string or map",
        )
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        FromStr::from_str(&value.to_string())
            .map(|e| vec![e])
            .map_err(de::Error::custom)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        FromStr::from_str(value)
            .map(|e| vec![e])
            .map_err(de::Error::custom)
    }

    fn visit_map<M>(self, visitor: M) -> Result<Self::Value, M::Error>
    where
        M: de::MapAccess<'de>,
    {
        de::Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))
            .map(|e| vec![e])
    }

    fn visit_seq<S>(self, mut seq: S) -> Result<Self::Value, S::Error>
    where
        S: de::SeqAccess<'de>,
    {
        let mut vec = Vec::new();
        while let Some(element) = seq.next_element_seed(StringOrStruct(PhantomData))? {
            vec.push(element);
        }
        Ok(vec)
    }
}

fn single_or_seq_string_or_struct<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    T: de::Deserialize<'de> + FromStr<Err = String>,
    D: de::Deserializer<'de>,
{
    deserializer.deserialize_any(SingleOrSeqStringOrStruct(PhantomData))
}

fn string_or_seq_string<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct StringOrSeqString(PhantomData<Vec<String>>);

    impl<'de> de::Visitor<'de> for StringOrSeqString {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or sequence of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![value.to_owned()])
        }

        fn visit_seq<S>(self, visitor: S) -> Result<Self::Value, S::Error>
        where
            S: de::SeqAccess<'de>,
        {
            de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(visitor))
        }
    }

    deserializer.deserialize_any(StringOrSeqString(PhantomData))
}

fn option_string_or_seq_string<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: de::Deserializer<'de>,
{
    string_or_seq_string(deserializer).map(Some)
}

fn struct_or_seq_struct<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    T: de::Deserialize<'de>,
    D: de::Deserializer<'de>,
{
    struct StructOrSeqStruct<T>(PhantomData<Vec<T>>);

    impl<'de, T> de::Visitor<'de> for StructOrSeqStruct<T>
    where
        T: de::Deserialize<'de>,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("map or sequence of maps")
        }

        fn visit_map<M>(self, visitor: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            de::Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))
                .map(|e| vec![e])
        }

        fn visit_seq<S>(self, visitor: S) -> Result<Self::Value, S::Error>
        where
            S: de::SeqAccess<'de>,
        {
            de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(visitor))
        }
    }

    deserializer.deserialize_any(StructOrSeqStruct(PhantomData))
}

fn option_struct_or_seq_struct<'de, T, D>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    T: de::Deserialize<'de>,
    D: de::Deserializer<'de>,
{
    struct_or_seq_struct(deserializer).map(Some)
}
