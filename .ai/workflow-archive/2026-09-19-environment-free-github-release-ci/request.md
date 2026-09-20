# Environment-Free GitHub Release CI

Create a GitHub pipeline that verifies a release build and runs all tests that do not require simulators, server infrastructure, or other environment setup.

## Revision 2

- The pipeline must run `make check`.
- The pipeline must run for every pull request and after changes are merged to `main`.
