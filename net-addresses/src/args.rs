use clap::{Parser, ArgGroup, value_parser};
use net_addresses::getaddrinfo::{AddrFamily, SockType, Protocol};

#[derive(Parser, Debug)]
#[command(
    version = "1.0",
    about = "CLI tool for resolving network addresses and services",
    group = ArgGroup::new("target").required(true).multiple(true).args(&["host", "service"])
)]
pub struct CliArgs {
    /// IPv4, IPv6, or domain name (e.g., 8.8.4.4, ::1, example.com)
    #[arg(short = 'H', long = "host", value_name = "IP / DOMAIN")]
    pub host: Option<String>,

    /// Port number or service name (e.g., 80, http)
    #[arg(short = 'S', long = "service", value_name = "PORT / SERVICE")]
    pub service: Option<String>,

    /// Filter by address family
    #[arg(short = 'f', long = "family", default_value = "unspecified")]
    pub family: AddrFamily,

    /// Filter by socket type
    #[arg(short = 't', long = "socktype", default_value = "unspecified")]
    pub socktype: SockType,

    /// Filter by transport protocol
    #[arg(short = 'p', long = "protocol", default_value = "unspecified")]
    pub protocol: Protocol,

    /// Resolve canonical name (Hostname must be provided)
    #[arg(short = 'c', long = "canonname", requires = "host")]
    pub canonname: bool,

    /// Verbose output level (0-2)
    #[arg(short = 'v', long = "verbose", name = "LEVEL", value_parser = value_parser!(u8).range(0..=2), default_value = "0")]
    pub verbose: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::{Error, ErrorKind};

    #[test]
    fn test_cliargs_fails_without_arguments() {
        // GIVEN
        let argv: [&str; 0] = [];
        // WHEN
        let result: Result<CliArgs, Error> = CliArgs::try_parse_from(&argv);
        // THEN
        assert!(result.is_err_and(|e| e.kind() == ErrorKind::MissingRequiredArgument));
    }

    #[test]
    fn test_cliargs_fails_with_service_and_canonname_missing_host() {
        // GIVEN
        let argv: [&str; 4] = ["--", "--service", "http", "--canonname"];
        // WHEN
        let result: Result<CliArgs, Error> = CliArgs::try_parse_from(&argv);
        // THEN
        assert!(result.is_err_and(|e| e.kind() == ErrorKind::MissingRequiredArgument));
    }

    #[test]
    fn test_cliargs_fails_with_invalid_family() {
        // GIVEN
        let argv: [&str; 5] = ["--", "-H", "127.0.0.1", "-f", "invalid"];
        // WHEN
        let result: Result<CliArgs, Error> = CliArgs::try_parse_from(&argv);
        // THEN
        assert!(result.is_err_and(|e| e.kind() == ErrorKind::InvalidValue));
    }

    #[test]
    fn test_cliargs_fails_with_out_of_range_verbose_value() {
        // GIVEN
        let argv: [&str; 5] = ["--", "-H", "1.1.1.1", "-v", "3"];
        // WHEN
        let result: Result<CliArgs, Error> = CliArgs::try_parse_from(&argv);
        // THEN
        assert!(result.is_err_and(|e| e.kind() == ErrorKind::ValueValidation));
    }

    #[test]
    fn test_cliargs_parses_host_only() {
        // GIVEN
        let argv: [&str; 3] = ["--", "--host", "8.8.4.4"];
        // WHEN
        let args: CliArgs = CliArgs::parse_from(&argv);
        // THEN
        assert!(args.host.is_some_and(|h| h == "8.8.4.4"));
        assert!(args.service.is_none());
    }

    #[test]
    fn test_cliargs_parses_service_only() {
        // GIVEN
        let argv: [&str; 3] = ["--", "--service", "443"];
        // WHEN
        let args: CliArgs = CliArgs::parse_from(&argv);
        // THEN
        assert!(args.host.is_none());
        assert!(args.service.is_some_and(|s| s == "443"));
    }

    #[test]
    fn test_cliargs_parses_host_and_service() {
        // GIVEN
        let argv: [&str; 5] = ["--", "-H", "dns.google", "-S", "https"];
        // WHEN
        let args: CliArgs = CliArgs::parse_from(&argv);
        // THEN
        assert!(args.host.is_some_and(|h| h == "dns.google"));
        assert!(args.service.is_some_and(|s| s == "https"));
        assert!(
            args.family == AddrFamily::Unspecified
                && args.socktype == SockType::Unspecified
                && args.protocol == Protocol::Unspecified
                && args.verbose == 0
                && !args.canonname
        );
    }

    #[test]
    #[rustfmt::skip]
    fn test_cliargs_parses_family_socktype_protocol() {
        // GIVEN
        let argv: [&str; 9] = ["--", "-H", "127.0.0.1", "-f", "inet6", "-t", "stream", "-p", "tcp"];
        // WHEN
        let args: CliArgs = CliArgs::parse_from(&argv);
        // THEN
        assert!(
            args.family == AddrFamily::Inet6
                && args.socktype == SockType::Stream
                && args.protocol == Protocol::Tcp
        );
    }
}
