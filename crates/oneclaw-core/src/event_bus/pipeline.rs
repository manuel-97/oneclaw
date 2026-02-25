//! Pipeline Engine — Declarative event processing chains
//!
//! A Pipeline is a series of steps: Filter -> Transform -> Action
//! Example: sensor.temp > 38.0 -> set priority Critical -> publish alert

use crate::event_bus::traits::{Event, EventPriority};
use tracing::debug;

/// Filter operation for pipeline steps
#[derive(Debug, Clone)]
pub enum FilterOp {
    /// Check if a data field exists
    HasField(String),
    /// Check if data field equals a value
    FieldEquals(String, String),
    /// Check if numeric data field is greater than threshold
    FieldGreaterThan(String, f64),
    /// Check if numeric data field is less than threshold
    FieldLessThan(String, f64),
    /// Topic matches pattern
    TopicMatches(String),
    /// Always pass
    Always,
}

impl FilterOp {
    /// Check if the given event matches this filter operation.
    pub fn matches(&self, event: &Event) -> bool {
        match self {
            Self::HasField(key) => event.data.contains_key(key),
            Self::FieldEquals(key, val) => event.data.get(key).map(|v| v == val).unwrap_or(false),
            Self::FieldGreaterThan(key, threshold) => {
                event.data.get(key)
                    .and_then(|v| v.parse::<f64>().ok())
                    .is_some_and(|v| v > *threshold)
            }
            Self::FieldLessThan(key, threshold) => {
                event.data.get(key)
                    .and_then(|v| v.parse::<f64>().ok())
                    .is_some_and(|v| v < *threshold)
            }
            Self::TopicMatches(pattern) => {
                if let Some(prefix) = pattern.strip_suffix('*') {
                    event.topic.starts_with(prefix)
                } else {
                    event.topic == *pattern
                }
            }
            Self::Always => true,
        }
    }
}

/// What to do when pipeline matches
#[derive(Debug, Clone)]
pub enum PipelineAction {
    /// Publish a new event with given topic, copying data from source
    EmitEvent {
        /// The topic for the emitted event.
        topic: String,
        /// The priority for the emitted event.
        priority: EventPriority,
    },
    /// Add/overwrite a data field
    SetField {
        /// The data field key to set.
        key: String,
        /// The data field value to set.
        value: String,
    },
    /// Set priority on the event
    SetPriority(EventPriority),
    /// Log a message (for debugging)
    Log(String),
}

/// A step in the pipeline
#[derive(Debug, Clone)]
pub struct PipelineStep {
    /// The name of this pipeline step.
    pub name: String,
    /// The filter that must match for actions to execute.
    pub filter: FilterOp,
    /// The actions to perform when the filter matches.
    pub actions: Vec<PipelineAction>,
}

/// A complete pipeline: named sequence of steps
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// The name of this pipeline.
    pub name: String,
    /// The topic pattern this pipeline subscribes to.
    pub topic_pattern: String,
    /// The ordered steps to execute for matching events.
    pub steps: Vec<PipelineStep>,
}

impl Pipeline {
    /// Create a new pipeline with the given name and topic pattern.
    pub fn new(name: impl Into<String>, topic_pattern: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            topic_pattern: topic_pattern.into(),
            steps: Vec::new(),
        }
    }

    /// Add a step to this pipeline (builder pattern).
    pub fn add_step(mut self, step: PipelineStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Process an event through this pipeline
    /// Returns: list of new events to emit
    pub fn process(&self, event: &Event) -> Vec<Event> {
        let mut emitted: Vec<Event> = Vec::new();
        let mut modified_event = event.clone();

        for step in &self.steps {
            if !step.filter.matches(&modified_event) {
                debug!(pipeline = %self.name, step = %step.name, "Filter did not match, skipping step");
                continue;
            }

            debug!(pipeline = %self.name, step = %step.name, "Step matched, executing actions");

            for action in &step.actions {
                match action {
                    PipelineAction::EmitEvent { topic, priority } => {
                        let mut new_event = Event::new(topic.clone(), format!("pipeline:{}", self.name));
                        new_event.data = modified_event.data.clone();
                        new_event.priority = *priority;
                        emitted.push(new_event);
                    }
                    PipelineAction::SetField { key, value } => {
                        modified_event.data.insert(key.clone(), value.clone());
                    }
                    PipelineAction::SetPriority(p) => {
                        modified_event.priority = *p;
                    }
                    PipelineAction::Log(msg) => {
                        tracing::info!(pipeline = %self.name, "{}", msg);
                    }
                }
            }
        }

        emitted
    }

    /// Check if this pipeline should handle the given topic
    pub fn matches_topic(&self, topic: &str) -> bool {
        if let Some(prefix) = self.topic_pattern.strip_suffix('*') {
            topic.starts_with(prefix)
        } else {
            self.topic_pattern == topic
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_field_greater_than() {
        let filter = FilterOp::FieldGreaterThan("temp".into(), 38.0);
        let event = Event::new("sensor.temp", "test").with_data("temp", "38.5");
        assert!(filter.matches(&event));

        let normal = Event::new("sensor.temp", "test").with_data("temp", "37.0");
        assert!(!filter.matches(&normal));
    }

    #[test]
    fn test_filter_field_equals() {
        let filter = FilterOp::FieldEquals("type".into(), "temperature".into());
        let event = Event::new("sensor", "test").with_data("type", "temperature");
        assert!(filter.matches(&event));
    }

    #[test]
    fn test_filter_has_field() {
        let filter = FilterOp::HasField("device".into());
        let event = Event::new("test", "test").with_data("device", "sensor_01");
        assert!(filter.matches(&event));

        let no_device = Event::new("test", "test");
        assert!(!filter.matches(&no_device));
    }

    #[test]
    fn test_filter_field_less_than() {
        let filter = FilterOp::FieldLessThan("humidity".into(), 30.0);
        let event = Event::new("sensor.humidity", "test").with_data("humidity", "25");
        assert!(filter.matches(&event));

        let normal = Event::new("sensor.humidity", "test").with_data("humidity", "55");
        assert!(!filter.matches(&normal));
    }

    #[test]
    fn test_filter_always() {
        let filter = FilterOp::Always;
        let event = Event::new("anything", "test");
        assert!(filter.matches(&event));
    }

    #[test]
    fn test_pipeline_threshold_detection() {
        let pipeline = Pipeline::new("high-temp-detect", "sensor.temp*")
            .add_step(PipelineStep {
                name: "check-high-temp".into(),
                filter: FilterOp::FieldGreaterThan("value".into(), 100.0),
                actions: vec![
                    PipelineAction::SetField {
                        key: "alert_type".into(),
                        value: "high_temperature".into(),
                    },
                    PipelineAction::EmitEvent {
                        topic: "alert.high_temp".into(),
                        priority: EventPriority::Critical,
                    },
                ],
            });

        // High value event
        let event = Event::new("sensor.temperature", "temp-sensor")
            .with_data("device", "device_01")
            .with_data("value", "105.3");

        let emitted = pipeline.process(&event);
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].topic, "alert.high_temp");
        assert_eq!(emitted[0].priority, EventPriority::Critical);
        assert_eq!(emitted[0].data.get("device"), Some(&"device_01".to_string()));
        assert_eq!(emitted[0].data.get("alert_type"), Some(&"high_temperature".to_string()));
    }

    #[test]
    fn test_pipeline_no_match() {
        let pipeline = Pipeline::new("high-temp-detect", "sensor.temp*")
            .add_step(PipelineStep {
                name: "check-high-temp".into(),
                filter: FilterOp::FieldGreaterThan("value".into(), 100.0),
                actions: vec![
                    PipelineAction::EmitEvent {
                        topic: "alert.high_temp".into(),
                        priority: EventPriority::Critical,
                    },
                ],
            });

        // Normal value — filter doesn't match
        let event = Event::new("sensor.temperature", "temp-sensor")
            .with_data("value", "22.5");

        let emitted = pipeline.process(&event);
        assert!(emitted.is_empty());
    }

    #[test]
    fn test_pipeline_multi_step() {
        let pipeline = Pipeline::new("pressure-monitor", "sensor.pressure*")
            .add_step(PipelineStep {
                name: "tag-high-pressure".into(),
                filter: FilterOp::FieldGreaterThan("value".into(), 1000.0),
                actions: vec![
                    PipelineAction::SetField { key: "risk".into(), value: "high".into() },
                ],
            })
            .add_step(PipelineStep {
                name: "emit-alert-if-risky".into(),
                filter: FilterOp::FieldEquals("risk".into(), "high".into()),
                actions: vec![
                    PipelineAction::EmitEvent {
                        topic: "alert.high_pressure".into(),
                        priority: EventPriority::High,
                    },
                ],
            });

        let event = Event::new("sensor.pressure", "gauge_01")
            .with_data("value", "1050")
            .with_data("unit", "hPa")
            .with_data("device", "sensor_03");

        let emitted = pipeline.process(&event);
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].topic, "alert.high_pressure");
        assert_eq!(emitted[0].data.get("device"), Some(&"sensor_03".to_string()));
    }

    #[test]
    fn test_pipeline_topic_matching() {
        let p = Pipeline::new("test", "sensor.*");
        assert!(p.matches_topic("sensor.temp"));
        assert!(p.matches_topic("sensor.motion"));
        assert!(!p.matches_topic("alert.threshold"));
    }
}
