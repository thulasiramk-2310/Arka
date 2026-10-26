cmake_minimum_required(VERSION 3.20)
project(arka-edit LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.6 REQUIRED COMPONENTS Gui Quick QuickControls2)

qt_add_executable(arka-edit main.cpp)
qt_add_resources(arka-edit "qml" PREFIX "/" FILES Edit.qml)
target_link_libraries(arka-edit PRIVATE Qt6::Gui Qt6::Quick Qt6::QuickControls2)
install(TARGETS arka-edit DESTINATION bin)
