import uvicorn
from fastapi import FastAPI, HTTPException, Depends, status
from pydantic import BaseModel
from typing import List, Optional
import pytest
import httpx

app = FastAPI()

class User(BaseModel):
    id: int
    name: str
    email: str
    age: int

users = []

def get_user_by_id(user_id: int):
    for user in users:
        if user.id == user_id:
            return user
    return None

@app.get("/users")
def get_users():
    return users

@app.get("/users/{user_id}")
def get_user(user_id: int):
    user = get_user_by_id(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")
    return user

@app.post("/users")
def create_user(user: User):
    users.append(user)
    return {"message": "User created", "user": user}

@app.put("/users/{user_id}")
def update_user(user_id: int, user: User):
    existing_user = get_user_by_id(user_id)
    if not existing_user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")
    existing_user.name = user.name
    existing_user.email = user.email
    existing_user.age = user.age
    return {"message": "User updated", "user": existing_user}

@app.delete("/users/{user_id}")
def delete_user(user_id: int):
    user = get_user_by_id(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")
    users.remove(user)
    return {"message": "User deleted"}

def test_get_users():
    with httpx.Client(app=app) as client:
        response = client.get("http://localhost:8000/users")
        assert response.status_code == 200
        assert isinstance(response.json(), list)

def test_get_user():
    with httpx.Client(app=app) as client:
        response = client.get("http://localhost:8000/users/1")
        assert response.status_code == 404

def test_create_user():
    with httpx.Client(app=app) as client:
        response = client.post("http://localhost:8000/users", json={"id": 1, "name": "John", "email": "john@example.com", "age": 30})
        assert response.status_code == 200
        assert response.json()["user"]["name"] == "John"

def test_update_user():
    with httpx.Client(app=app) as client:
        response = client.put("http://localhost:8000/users/1", json={"id": 1, "name": "Jane", "email": "jane@example.com", "age": 25})
        assert response.status_code == 200
        assert response.json()["user"]["name"] == "Jane"

def test_delete_user():
    with httpx.Client(app=app) as client:
        response = client.delete("http://localhost:8000/users/1")
        assert response.status_code == 200
        assert response.json()["message"] == "User deleted"