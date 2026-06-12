cmake_minimum_required(VERSION 3.22)

project(kore_browser LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_custom_target(rust-workspace
    COMMAND cargo build --workspace
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    COMMENT "Building Rust workspace"
)

enable_testing()

add_test(
    NAME rust-unit-tests
    COMMAND cargo test --workspace
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
)
