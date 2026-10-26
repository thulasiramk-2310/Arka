# Builds both Qt programs for the RPM (packaging/arka-convert.spec). The image
# build compiles each on its own, from its own folder.
cmake_minimum_required(VERSION 3.20)
project(arka-convert LANGUAGES CXX)
add_subdirectory(wheel)
add_subdirectory(edit)
