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
        num = int(input("Enter a number to calculate Fibonacci: "))
        result = fibonacci(num)
        print(f"Fibonacci({num}) = {result}")
    except Exception as e:
        print(f"Error: {str(e)}")