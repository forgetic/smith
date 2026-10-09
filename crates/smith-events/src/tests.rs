use crate::{Error, Limits, read};

#[test]
fn another_version_is_named() {
    let limits = Limits { string: 64, content: 1024, items: 4 };
    assert_eq!(read(b"{\"v\":2,\"type\":\"future\",\"t_ms\":0}\n", &limits), Err(Error::Version(2)));
}
