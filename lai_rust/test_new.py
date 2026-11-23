import unittest

def factorial(n):
    if not isinstance(n, int):
        raise ValueError("Input must be an integer")
    if n < 0:
        raise ValueError("Input must be a non-negative integer")
    result = 1
    for i in range(1, n + 1):
        result *= i
    return result

class TestFactorial(unittest.TestCase):
    def test_positive_numbers(self):
        self.assertEqual(factorial(5), 120)
        self.assertEqual(factorial(1), 1)
        self.assertEqual(factorial(0), 1)

    def test_negative_numbers(self):
        with self.assertRaises(ValueError):
            factorial(-1)
        with self.assertRaises(ValueError):
            factorial(-5)

    def test_non_integer_inputs(self):
        with self.assertRaises(ValueError):
            factorial(3.5)
        with self.assertRaises(ValueError):
            factorial("5")