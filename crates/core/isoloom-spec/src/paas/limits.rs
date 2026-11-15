//! What one cloud's managed service can't do that the spec asks for, as typed `refused` problems
//! at the field that asks it, each with its fix: that cloud of `cloud-vm` is refused with the
//! problem as its reason, the others still render. Event Hubs Standard (Azure's Kafka) compacts
//! no topic and takes at most [`EVENT_HUBS_MAX_PARTITIONS`] partitions a topic; MSK has no topic
//! resource, so its topics are made at their first use, all with one partition count, and none
//! compacted.

use super::model::{Cleanup, Service};
use crate::syntax::{Code, FieldPath, Problem};

/// The partitions an Event Hubs Standard hub takes at most.
pub const EVENT_HUBS_MAX_PARTITIONS: u16 = 32;

/// A cloud's refusal of a service, at the field that rules it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudLimit {
    /// The cloud, as `cloud-vm` names it (`aws`, `azure`).
    pub cloud: &'static str,
    /// Why, where, and what to do instead.
    pub problem: Problem,
}

/// The limits of the clouds service `name` (`service`) runs into.
#[must_use]
pub fn cloud_limits(name: &str, service: &Service) -> Vec<CloudLimit> {
    let Service::Kafka(k) = service else { return Vec::new() };
    let at = FieldPath::new("services").key(name).key("topics");
    let mut out = Vec::new();
    let mut limit = |cloud, problem| out.push(CloudLimit { cloud, problem });
    for (topic, t) in &k.topics {
        if t.cleanup == Cleanup::Compact {
            limit(
                "azure",
                Problem::new(
                    Code::Refused,
                    at.key(topic).key("cleanup"),
                    "Event Hubs Standard compacts no topic (Premium and Dedicated do)",
                )
                .fix("`cleanup: delete`, or another cloud"),
            );
            limit(
                "aws",
                Problem::new(
                    Code::Refused,
                    at.key(topic).key("cleanup"),
                    "MSK makes its topics at their first use, so none is compacted",
                )
                .fix("`cleanup: delete`, or another cloud"),
            );
        }
        if t.partitions > EVENT_HUBS_MAX_PARTITIONS {
            limit(
                "azure",
                Problem::new(
                    Code::Refused,
                    at.key(topic).key("partitions"),
                    format!("Event Hubs Standard takes at most {EVENT_HUBS_MAX_PARTITIONS} partitions a topic"),
                )
                .fix(format!("`partitions: {EVENT_HUBS_MAX_PARTITIONS}` or fewer, or another cloud")),
            );
        }
    }
    let mut counts: Vec<u16> = k.topics.values().map(|t| t.partitions).collect();
    counts.dedup();
    counts.sort_unstable();
    counts.dedup();
    if counts.len() > 1 {
        limit(
            "aws",
            Problem::new(
                Code::Refused,
                at,
                "MSK makes every topic at its first use with one partition count, and these topics differ",
            )
            .fix("give every topic the same `partitions`, or another cloud"),
        );
    }
    out
}
