pub use macros::{MCPResource, MCPTool};
pub use registry;

#[cfg(test)]
mod tests {
    use super::MCPTool;
    use serde::{Deserialize, Serialize};
    use serde_json::{Map, Value, json};

    #[derive(MCPTool, Deserialize, Serialize)]
    #[meta(title = "title")]
    struct ABC {
        test: u32,
        optional: Option<i16>,
    }

    #[test]
    fn it_works() {
        assert_eq!(super::registry::registry().tools().unwrap().len(), 1);
        assert!(super::registry::registry().resources().is_none());
        //println!("{}", ABC::params());
    }
}
