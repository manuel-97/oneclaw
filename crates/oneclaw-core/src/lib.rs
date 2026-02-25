#![warn(missing_docs)]
//! OneClaw Core — 5-layer trait-driven AI Agent Kernel
//!
//! Architecture:
//! - Layer 0: Security Core (Immune System)
//! - Layer 1: LLM Orchestrator (Heart) - MOAT
//! - Layer 2: Memory (Brain)
//! - Layer 3: Event Bus (Nervous System)
//! - Layer 4: Tool (Hands)
//! - Layer 5: Channel (Interface)

pub mod error;
pub mod config;
pub mod security;
pub mod orchestrator;
pub mod provider;
pub mod memory;
pub mod event_bus;
pub mod tool;
pub mod channel;
pub mod metrics;
pub mod registry;
pub mod runtime;
