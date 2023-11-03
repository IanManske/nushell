use ical::parser::ical::component::*;
use ical::property::Property;
use indexmap::map::IndexMap;
use nu_plugin::{EvaluatedCall, LabeledError};
use nu_protocol::{record, PluginExample, ShellError, Span, Value};
use std::io::BufReader;

pub const CMD_NAME: &str = "from ics";

pub fn from_ics_call(call: &EvaluatedCall, input: &Value) -> Result<Value, LabeledError> {
    let span = input.span();
    let input_string = input.as_string()?;
    let head = call.head;

    let input_string = input_string
        .lines()
        .enumerate()
        .map(|(i, x)| {
            if i == 0 {
                x.trim().to_string()
            } else if x.len() > 1 && (x.starts_with(' ') || x.starts_with('\t')) {
                x[1..].trim_end().to_string()
            } else {
                format!("\n{}", x.trim())
            }
        })
        .collect::<String>();

    let input_bytes = input_string.as_bytes();
    let buf_reader = BufReader::new(input_bytes);
    let parser = ical::IcalParser::new(buf_reader);

    let output = parser
        .into_iter()
        .map(|calendar| match calendar {
            Ok(c) => calendar_to_value(c, head),
            Err(e) => Value::error(
                ShellError::UnsupportedInput(
                    format!("input cannot be parsed as .ics ({e})"),
                    "value originates from here".into(),
                    head,
                    span,
                ),
                span,
            ),
        })
        .collect();

    Ok(Value::list(output, head))
}

pub fn examples() -> Vec<PluginExample> {
    vec![PluginExample {
        example: "'BEGIN:VCALENDAR
            END:VCALENDAR' | from ics"
            .into(),
        description: "Converts ics formatted string to table".into(),
        result: Some(Value::test_list(
            [Value::test_record(record! {
                    "properties" => Value::test_list([].into()),
                    "events" =>     Value::test_list([].into()),
                    "alarms" =>     Value::test_list([].into()),
                    "to-Dos" =>     Value::test_list([].into()),
                    "journals" =>   Value::test_list([].into()),
                    "free-busys" => Value::test_list([].into()),
                    "timezones" =>  Value::test_list([].into()),
            })]
            .into(),
        )),
    }]
}

fn calendar_to_value(calendar: IcalCalendar, span: Span) -> Value {
    Value::record(
        record! {
            "properties" => properties_to_value(calendar.properties, span),
            "events" => events_to_value(calendar.events, span),
            "alarms" => alarms_to_value(calendar.alarms, span),
            "to-Dos" => todos_to_value(calendar.todos, span),
            "journals" => journals_to_value(calendar.journals, span),
            "free-busys" => free_busys_to_value(calendar.free_busys, span),
            "timezones" => timezones_to_value(calendar.timezones, span),
        },
        span,
    )
}

fn events_to_value(events: Vec<IcalEvent>, span: Span) -> Value {
    Value::list(
        events
            .into_iter()
            .map(|event| {
                Value::record(
                    record! {
                        "properties" => properties_to_value(event.properties, span),
                        "alarms" => alarms_to_value(event.alarms, span),
                    },
                    span,
                )
            })
            .collect(),
        span,
    )
}

fn alarms_to_value(alarms: Vec<IcalAlarm>, span: Span) -> Value {
    Value::list(
        alarms
            .into_iter()
            .map(|alarm| {
                Value::record(
                    record! { "properties" => properties_to_value(alarm.properties, span), },
                    span,
                )
            })
            .collect(),
        span,
    )
}

fn todos_to_value(todos: Vec<IcalTodo>, span: Span) -> Value {
    Value::list(
        todos
            .into_iter()
            .map(|todo| {
                Value::record(
                    record! {
                        "properties" => properties_to_value(todo.properties, span),
                        "alarms" => alarms_to_value(todo.alarms, span),
                    },
                    span,
                )
            })
            .collect(),
        span,
    )
}

fn journals_to_value(journals: Vec<IcalJournal>, span: Span) -> Value {
    Value::list(
        journals
            .into_iter()
            .map(|journal| {
                Value::record(
                    record! { "properties" => properties_to_value(journal.properties, span), },
                    span,
                )
            })
            .collect(),
        span,
    )
}

fn free_busys_to_value(free_busys: Vec<IcalFreeBusy>, span: Span) -> Value {
    Value::list(
        free_busys
            .into_iter()
            .map(|free_busy| {
                Value::record(
                    record! { "properties" => properties_to_value(free_busy.properties, span) },
                    span,
                )
            })
            .collect(),
        span,
    )
}

fn timezones_to_value(timezones: Vec<IcalTimeZone>, span: Span) -> Value {
    Value::list(
        timezones
            .into_iter()
            .map(|timezone| {
                Value::record(
                    record! {
                        "properties" => properties_to_value(timezone.properties, span),
                        "transitions" => timezone_transitions_to_value(timezone.transitions, span),
                    },
                    span,
                )
            })
            .collect(),
        span,
    )
}

fn timezone_transitions_to_value(transitions: Vec<IcalTimeZoneTransition>, span: Span) -> Value {
    Value::list(
        transitions
            .into_iter()
            .map(|transition| {
                Value::record(
                    record! { "properties" => properties_to_value(transition.properties, span) },
                    span,
                )
            })
            .collect(),
        span,
    )
}

fn properties_to_value(properties: Vec<Property>, span: Span) -> Value {
    Value::list(
        properties
            .into_iter()
            .map(|prop| {
                let name = Value::string(prop.name, span);
                let value = match prop.value {
                    Some(val) => Value::string(val, span),
                    None => Value::nothing(span),
                };
                let params = match prop.params {
                    Some(param_list) => params_to_value(param_list, span),
                    None => Value::nothing(span),
                };

                Value::record(
                    record! {
                        "name" => name,
                        "value" => value,
                        "params" => params,
                    },
                    span,
                )
            })
            .collect(),
        span,
    )
}

fn params_to_value(params: Vec<(String, Vec<String>)>, span: Span) -> Value {
    let mut row = IndexMap::new();

    for (param_name, param_values) in params {
        let values = param_values
            .into_iter()
            .map(|val| Value::string(val, span))
            .collect();

        row.insert(param_name.into(), Value::list(values, span));
    }

    Value::record(row.into_iter().collect(), span)
}
