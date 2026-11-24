class Calculadora:

    def sumar(self, a, b):
        return a + b

    def restar(self, a, b):
        return a - b

    def multiplicar(self, a, b):
        return a * b

    def dividir(self, a, b):
        if b == 0:
            raise ValueError('Cannot divide by zero')
        return a / b

if __name__ == '__main__':
    calc = Calculadora()
    print(calc.sumar(5, 3))
    print(calc.restar(5, 3))
    print(calc.multiplicar(5, 3))
    print(calc.dividir(5, 3))