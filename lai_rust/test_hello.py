import unittest

def hola_mundo():
    try:
        return "hola mundo"
    except Exception as e:
        raise RuntimeError(f"Error en hola_mundo: {str(e)}")

class TestHolaMundo(unittest.TestCase):
    def test_hola_mundo(self):
        resultado = hola_mundo()
        self.assertEqual(resultado, "hola mundo", "El resultado no es 'hola mundo'")

if __name__ == '__main__':
    unittest.main()