#include <iostream>
#include <string>
#include <stdexcept>

using namespace std;

class Calculator {
public:
    double add(double a, double b) {
        return a + b;
    }

    double subtract(double a, double b) {
        return a - b;
    }

    double multiply(double a, double b) {
        return a * b;
    }

    double divide(double a, double b) {
        if (b == 0) {
            throw invalid_argument("Division by zero is not allowed.");
        }
        return a / b;
    }
};

int main() {
    Calculator calc;
    string operation;
    double num1, num2;

    cout << "Simple Calculator\n";
    cout << "Enter operation (+, -, *, /): ";
    cin >> operation;

    cout << "Enter first number: ";
    cin >> num1;

    cout << "Enter second number: ";
    cin >> num2;

    try {
        if (operation == "+") {
            cout << "Result: " << calc.add(num1, num2) << endl;
        } else if (operation == "-") {
            cout << "Result: " << calc.subtract(num1, num2) << endl;
        } else if (operation == "*") {
            cout << "Result: " << calc.multiply(num1, num2) << endl;
        } else if (operation == "/") {
            cout << "Result: " << calc.divide(num1, num2) << endl;
        } else {
            cout << "Invalid operation." << endl;
        }
    } catch (const invalid_argument& e) {
        cout << "Error: " << e.what() << endl;
    }

    return 0;
}