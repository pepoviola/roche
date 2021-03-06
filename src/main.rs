use anyhow::Result;
use cargo_generate::{generate, Args};
use clap::{App, Arg};
use dotenv;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::process;
use std::process::{Command, Stdio};

// Public Args look out for this PR landing to fix this
// https://github.com/ashleygwilliams/cargo-generate/pull/264

#[derive(Debug)]
pub struct PublicArgs {
    /// Git repository to clone template from. Can be a URL (like
    /// `https://github.com/rust-cli/cli-template`), a path (relative or absolute), or an
    /// `owner/repo` abbreviated GitHub URL (like `rust-cli/cli-template`).
    /// Note that cargo generate will first attempt to interpret the `owner/repo` form as a
    /// relative path and only try a GitHub URL if the local path doesn't exist.
    pub git: String,
    /// Branch to use when installing from git
    pub branch: Option<String>,
    /// Directory to create / project name; if the name isn't in kebab-case, it will be converted
    /// to kebab-case unless `--force` is given.
    pub name: Option<String>,
    /// Don't convert the project name to kebab-case before creating the directory.
    /// Note that cargo generate won't overwrite an existing directory, even if `--force` is given.
    pub force: bool,
    /// Enables more verbose output.
    pub verbose: bool,
}

impl Into<Args> for PublicArgs {
    fn into(self) -> Args {
        Args {
            git: Some(self.git),
            branch: self.branch,
            name: self.name,
            force: self.force,
            verbose: self.verbose,
            vcs: cargo_generate::Vcs::Git,

            // cargo_generate::Args doesn't implement Default
            list_favorites: false,
            favorite: None,
            template_values_file: None,
            silent: false,
            config: None,
        }
    }
}

pub fn generateimagetag(buildtype: String) -> Option<String> {
    let fullpath = match env::current_dir() {
        Err(why) => panic!("Couldn't get current dir {}", why),
        Ok(s) => s,
    };
    let pieces: Vec<&str> = fullpath
        .to_str()
        .unwrap()
        .split(std::path::MAIN_SEPARATOR)
        .collect();
    let mut dir = pieces[pieces.len() - 1];
    if dir == "src" {
        dir = pieces[pieces.len() - 2];
    }

    match getlogin() {
        Some(l) => {
            let img = format!("{}/{}{}", l, buildtype, dir);
            Some(img)
        }
        None => {
            let img = format!("{}{}", buildtype, dir);
            Some(img)
        }
    }
}

pub fn getdockerlogin() -> Option<String> {
    match env::var("DOCKER_USERNAME") {
        Ok(val) => return Some(val),
        Err(_e) => {
            println!("DOCKER_USERNAME environment variable not found trying to docker cli")
        }
    };

    let process = match Command::new("docker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .arg("info")
        .spawn()
    {
        Err(why) => {
            println!("couldn't spawn docker: {}", why);
            process::exit(1);
        }
        Ok(process) => process,
    };
    let mut s = String::new();
    let mut username: String = String::new();

    match process.stdout.unwrap().read_to_string(&mut s) {
        Err(why) => panic!("couldn't read docker stdout: {}", why),
        Ok(_) => {
            for line in s.lines() {
                if line.contains("Username") {
                    let vusername = line.split_whitespace();
                    username = vusername.last().unwrap_or_default().to_string();
                }
            }
        }
    };
    if username.is_empty() {
        None
    } else {
        Some(username)
    }
}

pub fn getlogin() -> Option<String> {
    match getdockerlogin() {
        Some(s) => Some(s),
        None => getpodmanlogin(),
    }
}

pub fn getpodmanlogin() -> Option<String> {
    let podman = match Command::new("podman")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .arg("login")
        .arg("--get-login")
        .spawn()
    {
        Err(why) => {
            println!("No Username found with docker or podman: {}", why);
            process::exit(1);
        }
        Ok(process) => process,
    };
    let mut podmanoutput = String::new();
    let username;
    match podman.stdout.unwrap().read_to_string(&mut podmanoutput) {
        Err(why) => panic!("couldn't read podman stdout: {}", why),
        Ok(_) => {
            username = podmanoutput.lines().next().unwrap_or_default().to_string();
        }
    }
    if username.is_empty() {
        None
    } else {
        Some(username)
    }
}
fn main() -> Result<()> {
    const FUNCTION: &str = include_str!("template/function.rs");
    const RELEASE_BUILD: &str = include_str!("template/Release.Dockerfile");
    const LOCAL_BUILD: &str = include_str!("template/Dev.Dockerfile");
    const TEST_BUILD: &str = include_str!("template/Libtest.Dockerfile");

    let dirname = env::current_dir().unwrap();
    let env_src_location = format!("{}/src/.rocherc", dirname.display());
    if Path::new(&env_src_location).exists() {
        dotenv::from_path(env_src_location).unwrap();
    }

    let env_top_location = format!("{}/.rocherc", dirname.display());
    if Path::new(&env_top_location).exists() {
        dotenv::from_path(env_top_location).unwrap();
    }

    let dev_build_image =
        env::var("dev_build_image").unwrap_or("quay.io/roche/dev-default:1.4.0".to_string());
    let test_build_image =
        env::var("test_build_image").unwrap_or("quay.io/roche/dev-default:1.4.0".to_string());
    let release_build_image =
        env::var("release_build_image").unwrap_or("quay.io/roche/default:1.4.0".to_string());
    let runtime_image =
        env::var("runtime_image").unwrap_or("quay.io/roche/alpine-libgcc:3.12".to_string());
    let default_project = "https://github.com/roche-rs/default";
    let mongodb_project = "https://github.com/roche-rs/mongodb";

    let matches = App::new("roche")
    .version("0.3.1")
    .author("Anton Whalley. <anton@venshare.com>")
    .about("A tool for building rust http and event services using containers")
        .subcommand(
            App::new("init").about("Generates a project").arg(
                Arg::new("template")
                    .about("template name 'default', 'mongodb' or git location. If not supplied a functions.rs file is generated.")
                    .index(1)
                    .required(false)
                    .multiple_values(true)
            )
            .arg(
                Arg::new("name")
                    .about("Name of the project")
                    .required(false)
                    .takes_value(true)
                    .short('n')
                    .long("name"),
            )
            .arg(
                Arg::new("branch")
                    .about("Branch to use when installing from git. Defaults to main.")
                    .required(false)
                    .takes_value(true)
                    .short('b')
                    .long("branch"),
            )
            .arg(
                Arg::new("force")
                    .about("Don't convert the project name to kebab-case before creating the directory.")
                    .required(false)
                    .takes_value(false)
                    .short('f')
                    .long("force"),
            )
            .arg(
                Arg::new("verbose")
                    .about("Enables more verbose output.")
                    .required(false)
                    .takes_value(true)
                    .short('v')
                    .long("verbose"),
            )
            ,
        ).subcommand(
            App::new("build").about("Builds a development image").arg(
                Arg::new("buildimage")
                    .about("buildimage to use. If not provided defaults to quay.io/roche/default:1.1.0")
                    .takes_value(true)
                    .short('b')
                    .long("buildimage")
                    .required(false)
            )
            .arg(
                Arg::new("runtimeimage")
                    .about("baseimage to use. If not provided defaults to quay.io/roche/alpine-libgcc:3.12")
                    .takes_value(true)
                    .short('r')
                    .long("runtime")
                    .required(false)
            )
            .arg(
                Arg::new("tag")
                    .about("tag for the build.")
                    .takes_value(true)
                    .short('t')
                    .long("tag")
                    .required(false)
            )
        )
        .subcommand(
            App::new("test").about("Runs the lib tests in an image").arg(
                Arg::new("libtestimage")
                    .about("Lib test image to use. If not provided defaults to quay.io/roche/default:1.2.0")
                    .takes_value(true)
                    .short('l')
                    .long("libtestimage")
                    .required(false)
            )
            .arg(
                Arg::new("tag")
                    .about("tag for the test run.")
                    .takes_value(true)
                    .short('t')
                    .long("tag")
                    .required(false)
            )
        )
        .subcommand(
            App::new("release").about("Builds a release image").arg(
                Arg::new("buildimage")
                    .about("buildimage to use. If not provided defaults to quay.io/roche/default:1.1.0")
                    .takes_value(true)
                    .short('b')
                    .long("buildimage")
                    .required(false)
            )
            .arg(
                Arg::new("runtimeimage")
                    .about("baseimage to use. If not provided defaults to quay.io/roche/alpine-libgcc:3.12")
                    .takes_value(true)
                    .short('r')
                    .long("runtime")
                    .required(false)
            )
            .arg(
                Arg::new("tag")
                    .about("tag for the build.")
                    .takes_value(true)
                    .short('t')
                    .long("tag")
                    .required(false)
            )
        ).subcommand(
            App::new("gen").about("Generates a release Dockerfile")
            .arg(
                Arg::new("buildimage")
                    .about("buildimage to use. If not provided defaults to quay.io/roche/default:1.1.0")
                    .takes_value(true)
                    .short('b')
                    .long("buildimage")
                    .required(false)
            )
            .arg(
                Arg::new("runtimeimage")
                    .about("baseimage to use. If not provided defaults to quay.io/roche/alpine-libgcc:3.12")
                    .takes_value(true)
                    .short('r')
                    .long("runtime")
                    .required(false)
            )
        )
        .get_matches();

    match matches.subcommand_name() {
        None => println!("No subcommand was used - try 'roche help'"),
        _ => {}
    };

    if matches.is_present("build") {
        // Check we have a functions.rs to build.
        let dirname = env::current_dir()?;

        let functionlocation = format!("{}/functions.rs", dirname.display());
        if !Path::new(&functionlocation).exists() {
            let srcfunction = format!("{}/src/functions.rs", dirname.display());
            if !Path::new(&srcfunction).exists() {
                println!(
                    "Cannot find functions.rs in the current folder or in src subfolder. Exiting"
                );
                process::exit(1);
            } else {
                let srcfolder = format!("{}/src", dirname.display());
                let srcpath = Path::new(&srcfolder);
                assert!(env::set_current_dir(&srcpath).is_ok());
            }
        }

        if let Some(build_matches) = matches.subcommand_matches("build") {
            let mut tag = format!("-t{}", build_matches.value_of("tag").unwrap_or(""));

            //let mut tag = build_matches.value_of("tag").unwrap_or("").to_string();

            if tag == "-t" {
                tag = match generateimagetag("dev-".to_string()) {
                    Some(s) => {
                        println!("No tag provided using {}", s);
                        format!("-t{}", s)
                    }
                    None => {
                        panic!("No tag provided and couldn't generate a tag. Please check you have logged into docker or podman")
                    }
                }
            }

            let buildimage = build_matches
                .value_of("buildimage")
                .unwrap_or(dev_build_image.as_str());
            let runtimeimage = build_matches
                .value_of("runtimeimage")
                .unwrap_or(runtime_image.as_str());
            let mut tmp_docker_file = str::replace(LOCAL_BUILD, "DEV_BASE_IMAGE", buildimage);
            tmp_docker_file = str::replace(tmp_docker_file.as_str(), "RUNTIME_IMAGE", runtimeimage);
            if Path::new(".env").exists() {
                tmp_docker_file = str::replace(
                    tmp_docker_file.as_str(),
                    "INCLUDE_ENV",
                    "app-build/src/.env*",
                );
            } else {
                tmp_docker_file = str::replace(tmp_docker_file.as_str(), "INCLUDE_ENV ", "");
            }
            let process = match Command::new("docker")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .arg("build")
                .arg(&tag)
                .arg("-f-")
                .arg(".")
                .spawn()
            {
                Err(why) => {
                    println!("couldn't spawn docker: {}", why);
                    process::exit(1);
                }
                Ok(process) => process,
            };

            match process.stdin.unwrap().write_all(tmp_docker_file.as_bytes()) {
                Err(why) => panic!("couldn't write to docker stdin: {}", why),
                Ok(_) => println!("Roche: Sent file to builder for {}", &tag),
            }
            let mut s = String::new();
            match process.stdout.unwrap().read_to_string(&mut s) {
                Err(why) => panic!("couldn't read docker stdout: {}", why),
                Ok(_) => print!("Roche: Build complete for {}\n{}", &tag, s),
            }
        }

        //
    }
    if matches.is_present("test") {
        // Check we have a functions.rs to test.
        let dirname = env::current_dir()?;

        let functionlocation = format!("{}/functions.rs", dirname.display());
        if !Path::new(&functionlocation).exists() {
            let srcfunction = format!("{}/src/functions.rs", dirname.display());
            if !Path::new(&srcfunction).exists() {
                println!(
                    "Cannot find functions.rs in the current folder or in src subfolder. Exiting"
                );
                process::exit(1);
            } else {
                let srcfolder = format!("{}/src", dirname.display());
                let srcpath = Path::new(&srcfolder);
                assert!(env::set_current_dir(&srcpath).is_ok());
            }
        }
        let dirname = env::current_dir()?;
        let testlocation = format!("{}/lib.rs", dirname.display());
        if !Path::new(&testlocation).exists() {
            println!("Cannot find lib.rs in the src folder. Exiting");
            process::exit(1);
        }

        if let Some(build_matches) = matches.subcommand_matches("test") {
            let mut tag = format!("-t{}", build_matches.value_of("tag").unwrap_or(""));

            //let mut tag = build_matches.value_of("tag").unwrap_or("").to_string();

            if tag == "-t" {
                tag = match generateimagetag("test-".to_string()) {
                    Some(s) => {
                        println!("No tag provided using {}", s);
                        format!("-t{}", s)
                    }
                    None => {
                        panic!("No tag provided and couldn't generate a tag. Please check you have logged into docker or podman")
                    }
                }
            }

            let testimage = build_matches
                .value_of("libtestimage")
                .unwrap_or(test_build_image.as_str());
            let mut tmp_docker_file = str::replace(TEST_BUILD, "TEST_BASE_IMAGE", testimage);
            if Path::new(".env").exists() {
                tmp_docker_file = str::replace(
                    tmp_docker_file.as_str(),
                    "INCLUDE_ENV",
                    "app-build/src/.env*",
                );
            } else {
                tmp_docker_file = str::replace(tmp_docker_file.as_str(), "INCLUDE_ENV ", "");
            }
            let process = match Command::new("docker")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .arg("build")
                .arg(&tag)
                .arg("-f-")
                .arg(".")
                .spawn()
            {
                Err(why) => {
                    println!("couldn't spawn docker: {}", why);
                    process::exit(1);
                }
                Ok(process) => process,
            };

            match process.stdin.unwrap().write_all(tmp_docker_file.as_bytes()) {
                Err(why) => panic!("couldn't write to docker stdin: {}", why),
                Ok(_) => println!("Roche: Sent file to builder for {}", &tag),
            }
            let mut s = String::new();
            match process.stdout.unwrap().read_to_string(&mut s) {
                Err(why) => panic!("couldn't read docker stdout: {}", why),
                Ok(_) => print!("Roche: Build complete for {}\n{}", &tag, s),
            }
        }
    }

    if matches.is_present("release") {
        // Check we have a functions.rs to build.
        println!("in release");
        let dirname = env::current_dir()?;

        let functionlocation = format!("{}/functions.rs", dirname.display());
        if !Path::new(&functionlocation).exists() {
            let srcfunction = format!("{}/src/functions.rs", dirname.display());
            if !Path::new(&srcfunction).exists() {
                println!(
                    "Cannot find functions.rs in the current folder or in src subfolder. Exiting"
                );
                process::exit(1);
            } else {
                let srcfolder = format!("{}/src", dirname.display());
                let srcpath = Path::new(&srcfolder);
                assert!(env::set_current_dir(&srcpath).is_ok());
            }
        }

        if let Some(build_matches) = matches.subcommand_matches("release") {
            let mut tag = format!("-t{}", build_matches.value_of("tag").unwrap_or(""));

            //let mut tag = build_matches.value_of("tag").unwrap_or("").to_string();

            if tag == "-t" {
                tag = match generateimagetag("".to_string()) {
                    Some(s) => {
                        println!("No tag provided using {}", s);
                        format!("-t{}", s)
                    }
                    None => {
                        panic!("No tag provided and couldn't generate a tag. Please check you have logged into docker or podman")
                    }
                }
            }

            let buildimage = build_matches
                .value_of("buildimage")
                .unwrap_or(release_build_image.as_str());
            let runtimeimage = build_matches
                .value_of("runtimeimage")
                .unwrap_or(runtime_image.as_str());

            let mut tmp_docker_file = str::replace(RELEASE_BUILD, "BASE_IMAGE", buildimage);

            if Path::new("lib.rs").exists() {
                tmp_docker_file = str::replace(
                    tmp_docker_file.as_str(),
                    "#LIB_RS",
                    "COPY lib.rs /app-build/src",
                );
                tmp_docker_file = str::replace(
                    tmp_docker_file.as_str(),
                    "#TEST",
                    "RUN cargo test --lib --release",
                );
            }
            if Path::new(".env").exists() {
                tmp_docker_file =
                    str::replace(tmp_docker_file.as_str(), "#ENV", "COPY .env /app-build/src");
            }

            tmp_docker_file = str::replace(tmp_docker_file.as_str(), "RUNTIME_IMAGE", runtimeimage);

            let process = match Command::new("docker")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .arg("build")
                .arg(&tag)
                .arg("-f-")
                .arg(".")
                .spawn()
            {
                Err(why) => {
                    println!("couldn't spawn docker: {}", why);
                    process::exit(1);
                }
                Ok(process) => process,
            };

            match process.stdin.unwrap().write_all(tmp_docker_file.as_bytes()) {
                Err(why) => panic!("couldn't write to docker stdin: {}", why),
                Ok(_) => println!("Roche: Sent file to builder for {}", &tag),
            }
            let mut s = String::new();
            match process.stdout.unwrap().read_to_string(&mut s) {
                Err(why) => panic!("couldn't read docker stdout: {}", why),
                Ok(_) => print!("Roche: Build complete for {}\n{}", &tag, s),
            }
        }
    }
    if matches.is_present("init") {
        if let Some(init_matches) = matches.subcommand_matches("init") {
            match init_matches.value_of("template").unwrap_or_default() {
                "default" => {
                    let name = init_matches.value_of("name").map(ToOwned::to_owned);
                    let branch = match init_matches.value_of("branch").map(ToOwned::to_owned) {
                        Some(b) => b,
                        None => String::from("main"),
                    };

                    let force: bool = init_matches.value_of_t("force").unwrap_or(false);
                    let verbose: bool = init_matches.value_of_t("verbose").unwrap_or(false);

                    let args_exposed: PublicArgs = PublicArgs {
                        git: default_project.to_string(),
                        branch: Some(branch),
                        name,
                        force,
                        verbose,
                    };

                    let args: Args = args_exposed.into();

                    generate(args)?
                }
                "mongodb" => {
                    let name = init_matches.value_of("name").map(ToOwned::to_owned);
                    let branch = match init_matches.value_of("branch").map(ToOwned::to_owned) {
                        Some(b) => b,
                        None => String::from("main"),
                    };

                    let force: bool = init_matches.value_of_t("force").unwrap_or(false);
                    let verbose: bool = init_matches.value_of_t("verbose").unwrap_or(false);

                    let args_exposed: PublicArgs = PublicArgs {
                        git: mongodb_project.to_string(),
                        branch: Some(branch),
                        name,
                        force,
                        verbose,
                    };

                    let args: Args = args_exposed.into();

                    generate(args)?
                }
                &_ => {
                    if init_matches
                        .value_of("template")
                        .unwrap_or_default()
                        .contains("https://")
                    {
                        let name = init_matches.value_of("name").map(ToOwned::to_owned);
                        let branch = match init_matches.value_of("branch").map(ToOwned::to_owned) {
                            Some(b) => b,
                            None => String::from("main"),
                        };

                        let force: bool = init_matches.value_of_t("force").unwrap_or(false);
                        let verbose: bool = init_matches.value_of_t("verbose").unwrap_or(false);

                        let args_exposed: PublicArgs = PublicArgs {
                            git: init_matches
                                .value_of("template")
                                .unwrap_or_default()
                                .to_string(),
                            branch: Some(branch),
                            name,
                            force,
                            verbose,
                        };
                        let args: Args = args_exposed.into();

                        generate(args)?
                    } else {
                        // init called but with no options so just generating a function.
                        let mut file = File::create("functions.rs")?;
                        file.write_all(FUNCTION.as_bytes())?
                    }
                }
            }
        }
    }
    if matches.is_present("gen") {
        if let Some(build_matches) = matches.subcommand_matches("gen") {
            let buildimage = build_matches
                .value_of("buildimage")
                .unwrap_or(release_build_image.as_str());
            let runtimeimage = build_matches
                .value_of("runtimeimage")
                .unwrap_or(runtime_image.as_str());
            let mut tmp_docker_file = str::replace(RELEASE_BUILD, "BASE_IMAGE", buildimage);
            tmp_docker_file = str::replace(tmp_docker_file.as_str(), "RUNTIME_IMAGE", runtimeimage);
            if Path::new("lib.rs").exists() {
                tmp_docker_file = str::replace(
                    tmp_docker_file.as_str(),
                    "#LIB_RS",
                    "COPY lib.rs /app-build/src",
                );
                tmp_docker_file = str::replace(
                    tmp_docker_file.as_str(),
                    "#TEST",
                    "RUN cargo test --lib --release",
                );
            }
            if Path::new(".env").exists() {
                tmp_docker_file =
                    str::replace(tmp_docker_file.as_str(), "#ENV", "COPY .env /app-build/src");
            }
            if !Path::new("Dockerfile").exists() {
                let mut file = File::create("Dockerfile")?;
                file.write_all(tmp_docker_file.as_bytes())?;
            } else {
                println!("Dockerfile already exists refusing to overwrite it. Please delete it and try again.");
            }
        }
    }
    Ok(())
}
