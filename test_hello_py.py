#!/usr/bin/env python3
"""Test file for hello world function."""

import unittest

def hola_mundo() -> str:
    """Returns a greeting message.

    Returns:
        A string containing the greeting "hola mundo".
    """
    return "hola mundo"

class TestHolaMundo(unittest.TestCase):
    """Test class for the hola_mundo function."""

    def test_hola_mundo(self) -> None:
        """Test that hola_mundo returns the correct greeting."""
        resultado = hola_mundo()
        self.assertEqual(resultado, "hola mundo")