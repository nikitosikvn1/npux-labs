#![allow(unused_imports)]
mod args;

use std::process;
use std::fmt::{Display, Debug};
use clap::Parser;
use args::CliArgs;

use libc::{AI_PASSIVE, AI_CANONNAME};
use net_addresses::getaddrinfo::{AddrInfo, AddrInfoHints};

// Returns a closure that prints items of type `T` in different formats depending on verbosity.
fn get_printer<T: Display + Debug + 'static>(verbosity: u8) -> impl Fn(&T) {
    move |item| match verbosity {
        0 => println!("{}", item),
        1 => println!("{:?}", item),
        2 => println!("{:#?}", item),
        _ => unreachable!(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(not(target_family = "unix")) {
        eprintln!("This program is intended for Unix-like systems only.");
        process::exit(1);
    }

    let args = dbg!(CliArgs::parse());

    let printer = get_printer(args.verbose);
    let hints = AddrInfoHints {
        flags: if args.canonname { AI_CANONNAME } else { 0 },
        family: args.family,
        socktype: args.socktype,
        protocol: args.protocol,
    };

    net_addresses::getaddrinfo(args.host.as_deref(), args.service.as_deref(), Some(hints))?
        .for_each(|ai_result| match ai_result {
            Ok(ai) => printer(&ai),
            Err(e) => eprintln!("Error resolving address: {:?}", e),
        });

    Ok(())
}
