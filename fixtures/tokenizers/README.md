# Tokenizer fixtures

`gpt2.json` is the `tokenizer.json` of [openai-community/gpt2](https://huggingface.co/openai-community/gpt2)
(MIT license), used to test Hugging Face tokenizer support without network access.

Reference counts in the tests come from the Python `tokenizers` package (0.23.3):

```python
from tokenizers import Tokenizer
t = Tokenizer.from_file("gpt2.json")
len(t.encode("Autenticação falhou 🚀", add_special_tokens=False).ids)  # 10
```
