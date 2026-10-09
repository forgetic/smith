//! Concrete accepted result decoding (protocol/charter.md, section 5;
//! protocol/channel.md, section 5). No state or policy is kept here.
use crate::Error;
use alloc::boxed::Box;
use skein_lib::{List, Reader};
use smith_domain::run;

/// Decode the accepted outcome body after the host channel has bounded it.
pub fn decode_declared(bytes: &[u8], limits: &smith_charter::v2::Limits) -> Result<run::outcome::Declared, Error> {
    let mut reader = Reader::new(bytes);
    let Ok(record) = smith_charter::RunResult::decode(limits, &mut reader) else {
        return Err(Error::InvalidResult);
    };
    if reader.remaining() != 0 {
        return Err(Error::InvalidResult);
    }
    let fields = decode_fields(record.fields())?;
    let declared = match record.form() {
        smith_charter::Form::Change => {
            if record.label().is_some() || !record.text().is_empty() || !record.items().is_empty() {
                return Err(Error::InvalidResult);
            }
            run::outcome::Declared::Change(run::outcome::Change { fields })
        }
        smith_charter::Form::Report => {
            if record.label().is_some() || !record.items().is_empty() {
                return Err(Error::InvalidResult);
            }
            run::outcome::Declared::Report(run::outcome::Report { text: Box::from(record.text()), fields })
        }
        smith_charter::Form::Failure => {
            if record.label().is_some() || !record.items().is_empty() {
                return Err(Error::InvalidResult);
            }
            run::outcome::Declared::Failure(run::outcome::DeclaredFailure { reason: Box::from(record.text()), fields })
        }
        smith_charter::Form::Verdict => {
            let Some(label) = record.label() else { return Err(Error::InvalidResult) };
            let mut items = List::with_capacity(record.items().len());
            for item in record.items() {
                let value = run::outcome::Item { kind: Box::from(item.kind()), fields: decode_fields(item.fields())? };
                if items.push(value).is_err() {
                    return Err(Error::InvalidResult);
                }
            }
            run::outcome::Declared::Verdict(run::outcome::Verdict {
                name: label.clone(),
                text: Box::from(record.text()),
                fields,
                items: items.into_boxed(),
            })
        }
    };
    Ok(declared)
}

fn decode_fields(source: &List<smith_charter::Field>) -> Result<Box<[run::outcome::Field]>, Error> {
    let mut fields = List::with_capacity(source.len());
    for field in source {
        let item = run::outcome::Field { name: Box::from(field.name()), value: Box::from(field.text()) };
        if fields.push(item).is_err() {
            return Err(Error::InvalidResult);
        }
    }
    Ok(fields.into_boxed())
}

#[cfg(test)]
mod tests {
    use super::decode_declared;
    use alloc::boxed::Box;
    use smith_domain::run;
    #[test]
    fn a_change_and_report_decode_from_the_agent_result_body() {
        let change = run::outcome::Declared::Change(run::outcome::Change {
            fields: Box::from([run::outcome::Field {
                name: Box::from(&b"title"[..]),
                value: Box::from(&b"Commit"[..]),
            }]),
        });
        let bytes = crate::encode_result(&change, &smith_charter::CEILINGS).expect("change bytes");
        assert_eq!(decode_declared(&bytes, &smith_charter::CEILINGS), Ok(change));
        let report = run::outcome::Declared::Report(run::outcome::Report {
            text: Box::from(&b"finished"[..]),
            fields: Box::new([]),
        });
        let bytes = crate::encode_result(&report, &smith_charter::CEILINGS).expect("report bytes");
        assert_eq!(decode_declared(&bytes, &smith_charter::CEILINGS), Ok(report));
    }
}
