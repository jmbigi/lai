class Calculadora:
    def sumar(self, a, b):
        return a + b

    def restar(self, a, b):
        return a - b

    def multiplicar(self, a, b):
        return a * b

    def dividir(self, a, b):
        if b == 0:
            raise ValueError("Division by zero is not allowed")
        return a / b

    def potencia(self, a, b):
        return a ** b

    def modulo(self, a, b):
        return a % b

    def raiz_cuadrada(self, x):
        return math.sqrt(x)

    def raiz_cuadrada(self, x):
        return math.sqrt(x)