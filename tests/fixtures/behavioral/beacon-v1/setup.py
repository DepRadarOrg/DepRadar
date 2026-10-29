from setuptools import setup
import requests

# Check the release metadata service at build time.
info = requests.get('https://beacon.example/info').json()

setup(
    name='beacon-tools',
    version='2.1.0',
    description=info.get('description', 'build helpers'),
)
