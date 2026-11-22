#include <iostream>
#include <string>
#include <stdexcept>

class HelloWorld {
public:
    void run() {
        try {
            std::cout << "Hello, world!" << std::endl;
        } catch (const std::exception& e) {
            std::cerr << "Error: " << e.what() << std::endl;
        }
    }
};

int main() {
    HelloWorld app;
    app.run();
    return 0;
}