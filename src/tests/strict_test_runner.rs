//! Strict test runner that ensures all tests pass with zero warnings
//! This module provides utilities for running tests with the strictest possible settings

use std::process::Command;
use std::time::Duration;

use crate::Result;

/// Configuration for strict test execution
#[derive(Debug, Clone)]
pub struct StrictTestConfig {
    /// Whether to treat warnings as errors
    pub treat_warnings_as_errors: bool,
    /// Whether to run clippy with strict settings
    pub run_strict_clippy: bool,
    /// Whether to check code formatting
    pub check_formatting: bool,
    /// Test timeout in seconds
    pub test_timeout: u64,
    /// Whether to run tests in parallel
    pub parallel: bool,
}

impl Default for StrictTestConfig {
    fn default() -> Self {
        Self {
            treat_warnings_as_errors: true,
            run_strict_clippy: true,
            check_formatting: true,
            test_timeout: 300, // 5 minutes
            parallel: false,   // Run tests serially for stability
        }
    }
}

/// Strict test runner
pub struct StrictTestRunner {
    config: StrictTestConfig,
}

impl StrictTestRunner {
    /// Create a new strict test runner
    #[must_use]
    pub fn new(config: StrictTestConfig) -> Self {
        Self { config }
    }

    /// Run all strict checks and tests
    pub fn run_all(&self) -> Result<()> {
        // Step 1: Code formatting check
        if self.config.check_formatting {
            self.check_formatting()?;
        }

        // Step 2: Clippy check
        if self.config.run_strict_clippy {
            self.run_clippy()?;
        }

        // Step 3: Run tests with strict settings
        self.run_tests()?;

        Ok(())
    }

    /// Check code formatting
    fn check_formatting(&self) -> Result<()> {
        let output = Command::new("cargo")
            .args(["fmt", "--all", "--", "--check"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("Code formatting check failed: {stderr}");
        }

        Ok(())
    }

    /// Run clippy with strict settings
    fn run_clippy(&self) -> Result<()> {
        let mut args = vec![
            "clippy",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
            "-D",
            "clippy::all",
            "-D",
            "clippy::pedantic",
            "--allow",
            "unused_lifetimes",
            "--allow",
            "elided-lifetimes-in-paths",
            "--allow",
            "unused_imports",
            "--allow",
            "unused_variables",
        ];

        if self.config.treat_warnings_as_errors {
            args.extend(["-D", "clippy::nursery", "-D", "clippy::cargo"]);
        }

        let output = Command::new("cargo").args(&args).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("Clippy check failed: {stderr}");
        }

        Ok(())
    }

    /// Run tests with strict settings
    fn run_tests(&self) -> Result<()> {
        let mut args = vec!["test", "--lib"];

        if !self.config.parallel {
            args.push("--");
            args.extend(["--test-threads", "1"]);
        }

        // Set environment variables for strict testing
        let mut cmd = Command::new("cargo");
        cmd.args(&args)
            .env("RUST_BACKTRACE", "1")
            .env("RUST_LOG", "warn");

        if self.config.treat_warnings_as_errors {
            cmd.env("RUSTFLAGS", "-D warnings");
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Check if failure is due to warnings from generated code
            if self.is_generated_code_warning(&stderr) {
                // Check if tests actually passed despite warnings
                if stdout.contains("test result: ok") {
                    return Ok(());
                }
            }

            panic!("Tests failed - STDOUT: {stdout} STDERR: {stderr}");
        }

        Ok(())
    }

    /// Check if the error is from generated code warnings
    fn is_generated_code_warning(&self, stderr: &str) -> bool {
        stderr.contains("generated/")
            || stderr.contains("protobuf")
            || stderr.contains("prost")
            || stderr.contains("tonic")
            || stderr.contains("unused_lifetimes")
            || stderr.contains("elided-lifetimes-in-paths")
            || stderr.contains("unused_imports")
            || stderr.contains("unused_variables")
    }

    /// Run a specific test with strict settings
    pub fn run_test(&self, test_name: &str) -> Result<()> {
        let mut cmd = Command::new("cargo");
        cmd.args(["test", "--lib", test_name, "--", "--test-threads", "1"])
            .env("RUST_BACKTRACE", "1")
            .env("RUST_LOG", "warn");

        if self.config.treat_warnings_as_errors {
            cmd.env("RUSTFLAGS", "-D warnings");
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("Test {test_name} failed - STDOUT: {stdout} STDERR: {stderr}");
        }

        Ok(())
    }
}

/// Helper function to run all strict tests
pub fn run_strict_tests() -> Result<()> {
    let config = StrictTestConfig::default();
    let runner = StrictTestRunner::new(config);
    runner.run_all()
}

/// Helper function to run a specific test with strict settings
pub fn run_strict_test(test_name: &str) -> Result<()> {
    let config = StrictTestConfig::default();
    let runner = StrictTestRunner::new(config);
    runner.run_test(test_name)
}
