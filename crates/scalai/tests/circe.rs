//! A script deriving Circe's `Codec` through its macros, compiled by the own
//! frontend with Circe from Coursier; skipped outside CI when Coursier cannot provide it.

use std::path::PathBuf;
use std::process::Command;

const DEPS: [&str; 2] = ["io.circe::circe-parser::0.14.10", "io.circe::circe-generic::0.14.10"];

const SCRIPT: &str = "import io.circe.Codec, io.circe.syntax.*, io.circe.parser.decode

case class Person(id: Int, name: String) derives Codec.AsObject

@main def run(): Unit =
  println(Person(1, \"a\").asJson.noSpaces)
  println(decode[Person](\"\"\"{\"id\":1,\"name\":\"a\"}\"\"\"))
";

#[test]
fn circe_codec_derives_in_a_script() {
    let deps: Vec<String> = DEPS.map(str::to_owned).to_vec();
    if let Err(e) = libs::jars().and_then(|_| libs::deps::fetch(&deps).map_err(|e| e.to_string()))
        && std::env::var_os("CI").is_none()
    {
        eprintln!("skipped: {e}");
        return;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("circe");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("circe.scala");
    std::fs::write(&file, SCRIPT).unwrap();
    let cache = dir.join("cache");
    let _ = std::fs::remove_dir_all(cache.join("scripts"));
    let out = Command::new(env!("CARGO_BIN_EXE_scalai"))
        .env("SCALAI_TIMEOUT", "300")
        .env("SCALAI_FRONTEND", "strict")
        .env("SCALAI_CACHE", &cache)
        .args(DEPS.iter().flat_map(|d| ["--dep", d]))
        .arg(&file)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected = "{\"id\":1,\"name\":\"a\"}\nRight(Person(1,a))\n";
    assert_eq!(stdout, expected, "{}", String::from_utf8_lossy(&out.stderr));
}
