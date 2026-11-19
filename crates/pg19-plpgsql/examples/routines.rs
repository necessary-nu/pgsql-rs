use pg19_plpgsql::{parse_routines, Options, Routine, RoutineError};
use serde_json::json;

fn main() {
    for sql in std::env::args().skip(1) {
        let output = match parse_routines(&sql, Options::default()) {
            Ok(routines) => {
                json!({"functions":routines.iter().map(Routine::json).collect::<Vec<_>>(),"error":null})
            }
            Err(e) => {
                let outer = matches!(e, RoutineError::OuterSql(_));
                if let Some((d, context)) = e.diagnostic() {
                    json!({"functions":null,"error":{"phase":if outer{"sql"}else{"plpgsql"},"message":d.message,"sqlstate":d.sqlstate,
                        "position":if outer{d.position}else{0},"internal_position":if outer{0}else{d.position},"context":context}})
                } else {
                    json!({"functions":null,"unsupported":format!("{e:?}")})
                }
            }
        };
        println!("{output}");
    }
}
