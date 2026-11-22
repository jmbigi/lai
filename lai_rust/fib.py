def fibonacci(n):
    if not isinstance(n, int):
        raise ValueError("Input must be an integer")
    if n < 0:
        raise ValueError("Input must be a non-negative integer")
    if n == 0:
        return 0
    elif n == 1:
        return 1
    else:
        return fibonacci(n - 1) + fibonacci(n - 2)

if __name__ == "__main__":
    try:
        num = int(input("Enter a non-negative integer: "))
        result = fibonacci(num)
        print(f"Fibonacci number at position {num} is {result}")
    except ValueError as e:
        print(f"Error: {e}")