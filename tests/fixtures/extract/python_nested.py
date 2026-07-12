class MyClass:
    def outer(self):
        pass

    def inner_method(self):
        """Nested method — regex misses this; tree-sitter should find it."""
        pass
